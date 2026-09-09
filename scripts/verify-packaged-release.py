#!/usr/bin/env python3
"""Exercise only staged release archives, using a fresh offline consumer home."""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.request

root = Path(__file__).resolve().parent.parent
proof = Path(sys.argv[1]).resolve()
cohort = json.loads((proof / "cohort.json").read_text())
version = cohort["version"]
with tempfile.TemporaryDirectory(prefix="nidus-artifact-consumers-") as temporary:
    work = Path(temporary)
    consumer_home = work / "cargo-home"
    consumer_home.mkdir()
    shutil.copyfile(proof / "cargo-home/config.toml", consumer_home / "config.toml")
    environment = dict(os.environ, CARGO_HOME=str(consumer_home), CARGO_NET_OFFLINE="true",
                       NIDUS_RELEASE_VERSION=version, NIDUS_EXTERNAL_EXAMPLES_LOCAL_PATCH="0",
                       NIDUS_CONSUMER_EVIDENCE_DIR=str(proof / "consumer-evidence"))
    # Never inherit a target directory that could contain workspace build artifacts.
    environment.pop("CARGO_TARGET_DIR", None)

    def run(arguments, **kwargs):
        print("[artifact-consumer]", " ".join(map(str, arguments)), flush=True)
        subprocess.run(list(map(str, arguments)), check=True, env=environment, **kwargs)

    smoke = work / "all-adapters"
    (smoke / "src").mkdir(parents=True)
    manifest = '[package]\nname="nidus-artifact-proof"\nversion="0.1.0"\nedition="2024"\n[dependencies]\n'
    for package in cohort["packages"]:
        if package["name"] == "cargo-nidus":
            continue
        alias = "nidus" if package["name"] == "nidus-rs" else package["name"]
        manifest += f'{alias} = {{ package = "{package["name"]}", version = "={version}", features = {json.dumps(package["features"])} }}\n'
    manifest += 'async-trait="0.1"\ntokio={version="1",features=["macros","rt-multi-thread"]}\n'
    (smoke / "Cargo.toml").write_text(manifest)
    (smoke / "src/main.rs").write_text('''#![forbid(unsafe_code)]
use nidus::prelude::*;
use nidus::{Module, ModuleBuilder, ModuleDefinition, Resource, Container};
use nidus::lifecycle::managed::ManagedOptions;
use std::sync::atomic::{AtomicBool, Ordering};
struct Owned(AtomicBool);
#[async_trait::async_trait]
impl Resource for Owned {
    async fn initialize(_: &Container) -> nidus::Result<Self> { Ok(Self(AtomicBool::new(false))) }
    async fn shutdown(&self) -> nidus::Result<()> { self.0.store(true, Ordering::SeqCst); Ok(()) }
}
#[controller("/proof")]
struct Controller { owned: Inject<Owned> }
#[routes]
impl Controller {
    #[get("/")]
    async fn proof(&self) -> &'static str {
        assert!(!self.owned.0.load(Ordering::SeqCst)); "archive-owned"
    }
}
struct Root;
impl Module for Root {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("Root").resource::<Owned>().controller_typed::<Controller>().build()
    }
}
#[tokio::main]
async fn main() {
    let managed = Nidus::create::<Root>().build_managed(ManagedOptions::default()).await.unwrap();
    let resource = managed.target().application().container().resolve::<Owned>().unwrap();
    let app = nidus_testing::TestApp::from_managed(managed);
    app.get("/proof").send().await.assert_text("archive-owned");
    assert!(app.shutdown_report().await.unwrap().is_success());
    assert!(resource.0.load(Ordering::SeqCst));
    println!("archive module/controller/resource/shutdown proof passed");
}
''')
    run(["cargo", "run", "--manifest-path", smoke / "Cargo.toml"])
    run(["python3", root / "scripts/check-consumer-cohort.py", smoke / "Cargo.toml", version, "registry"])

    with tarfile.open(proof / "registry" / f'cargo-nidus-{version}.crate') as archive:
        archive.extractall(work / "cli-source", filter="data")
    run(["cargo", "install", "--path", work / "cli-source" / f"cargo-nidus-{version}",
         "--root", work / "installed", "--locked", "--offline"])
    run([work / "installed/bin/cargo-nidus", "nidus", "new", "archive-generated", "--path", work])
    generated = work / "archive-generated"
    run(["cargo", "test", "--manifest-path", generated / "Cargo.toml"])
    run(["python3", root / "scripts/check-consumer-cohort.py", generated / "Cargo.toml", version, "registry"])
    run(["cargo", "build", "--locked", "--manifest-path", generated / "Cargo.toml"])
    with socket.socket() as allocator:
        allocator.bind(("127.0.0.1", 0))
        port = allocator.getsockname()[1]
    server_environment = dict(environment, NIDUS_ADDR=f"127.0.0.1:{port}")
    with (proof / "generated-server.log").open("w") as log:
        process = subprocess.Popen([generated / "target/debug/archive-generated"], env=server_environment,
                                   stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 30
            while True:
                assert process.poll() is None, "owned generated server exited before readiness"
                try:
                    with urllib.request.urlopen(f"http://127.0.0.1:{port}/", timeout=1) as response:
                        assert response.read() == b"hello from nidus"
                    break
                except OSError:
                    if time.monotonic() >= deadline:
                        raise
                    time.sleep(0.1)
        finally:
            if process.poll() is None:
                process.terminate()
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
                raise
    with socket.socket() as released:
        released.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        released.bind(("127.0.0.1", port))
    run(["bash", root / "scripts/verify-external-examples.sh"], cwd=root)
print("[artifact-consumer] all-adapter smoke, generated CLI and both standalone consumers passed; owned process and temporary files removed", flush=True)
