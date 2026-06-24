mod support;

use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use support::{temp_project_root, workspace_root};

#[test]
fn cargo_nidus_new_generates_compilable_nidus_project() {
    let root = temp_project_root("new_generates_compilable_nidus_project");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .status()
        .unwrap();

    assert!(status.success());
    assert!(project.join("Cargo.toml").exists());
    assert!(project.join("src/main.rs").exists());
    let cargo_toml = fs::read_to_string(project.join("Cargo.toml")).unwrap();
    assert!(!cargo_toml.contains("axum ="));
    assert!(!cargo_toml.contains("tokio ="));
    let main_rs = fs::read_to_string(project.join("src/main.rs")).unwrap();
    assert!(main_rs.contains("#[nidus::main]"));
    assert!(!main_rs.contains("#[tokio::main]"));
    assert!(main_rs.contains("#[controller(\"/\")]"));
    assert!(main_rs.contains("#[routes]"));
    assert!(main_rs.contains("#[get(\"/\")]"));
    assert!(main_rs.contains("HelloController.into_router()"));
    assert!(main_rs.contains("Nidus::bootstrap::<AppModule>()"));
    assert!(main_rs.contains("NIDUS_ADDR"));
    assert!(main_rs.contains("#[module]"));
    assert!(main_rs.contains("struct AppModule;"));
    assert!(!main_rs.contains("impl Module for AppModule"));
    assert!(main_rs.contains(".with_router("));
    assert!(main_rs.contains(".listen(address)"));
    assert!(
        fs::read_to_string(project.join("README.md"))
            .unwrap()
            .contains("hello-nidus")
    );
    assert!(
        fs::read_to_string(project.join("README.md"))
            .unwrap()
            .contains("NIDUS_ADDR")
    );

    let check = Command::new("cargo")
        .arg("check")
        .current_dir(&project)
        .status()
        .unwrap();
    assert!(check.success());
}

#[test]
fn cargo_nidus_new_generates_runnable_http_server() {
    let root = temp_project_root("new_generates_runnable_http_server");
    let project = root.join("hello-nidus");
    let status = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .status()
        .unwrap();
    assert!(status.success());

    let address = free_loopback_address();
    let child = Command::new("cargo")
        .arg("run")
        .env("NIDUS_ADDR", &address)
        .current_dir(&project)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard::new(child);

    let response = wait_for_http_response(&address, child.as_mut());
    child.stop();

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(response.contains("hello from nidus"), "{response}");
}

#[test]
fn cargo_nidus_new_refuses_to_overwrite_existing_project() {
    let root = temp_project_root("new_refuses_to_overwrite_existing_project");
    let project = root.join("hello-nidus");
    fs::create_dir_all(project.join("src")).unwrap();
    fs::write(project.join("Cargo.toml"), "# existing manifest\n").unwrap();
    fs::write(project.join("src/main.rs"), "// user edits\n").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_cargo-nidus"))
        .args(["nidus", "new", "hello-nidus", "--path"])
        .arg(&root)
        .arg("--nidus-path")
        .arg(workspace_root().join("crates/nidus"))
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("project already exists"));
    assert_eq!(
        fs::read_to_string(project.join("Cargo.toml")).unwrap(),
        "# existing manifest\n"
    );
    assert_eq!(
        fs::read_to_string(project.join("src/main.rs")).unwrap(),
        "// user edits\n"
    );
}

fn free_loopback_address() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().to_string()
}

fn wait_for_http_response(address: &str, child: &mut Child) -> String {
    let timeout = Duration::from_secs(90);
    let deadline = Instant::now() + timeout;
    let mut last_error = None;

    while Instant::now() < deadline {
        if let Some(status) = child.try_wait().unwrap() {
            panic!("generated server exited before responding: {status}");
        }

        match TcpStream::connect(address) {
            Ok(mut stream) => {
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                stream
                    .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                    .unwrap();
                let mut response = String::new();
                stream.read_to_string(&mut response).unwrap();
                return response;
            }
            Err(error) => {
                last_error = Some(error);
                thread::sleep(Duration::from_millis(100));
            }
        }
    }

    panic!(
        "generated server did not respond at {address} within {timeout:?}: {}",
        last_error
            .map(|error| error.to_string())
            .unwrap_or_else(|| "no connection attempt made".to_owned())
    );
}

struct ChildGuard {
    child: Child,
    stopped: bool,
}

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self {
            child,
            stopped: false,
        }
    }

    fn as_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    fn stop(mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        if self.stopped {
            return;
        }
        if self.child.try_wait().unwrap().is_none() {
            self.child.kill().unwrap();
        }
        let _ = self.child.wait();
        self.stopped = true;
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.stopped {
            return;
        }
        if !std::thread::panicking() {
            self.stop_inner();
        } else if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
