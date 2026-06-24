use std::sync::Arc;

use nidus_core::{ModuleBuilder, ModuleGraph};
use tracing::Level;
use tracing_subscriber::{Layer, fmt::MakeWriter, layer::SubscriberExt};

#[derive(Clone, Default)]
struct SharedLogWriter {
    output: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl SharedLogWriter {
    fn contents(&self) -> String {
        String::from_utf8(self.output.lock().unwrap().clone()).unwrap()
    }

    fn clear(&self) {
        self.output.lock().unwrap().clear();
    }
}

impl<'writer> MakeWriter<'writer> for SharedLogWriter {
    type Writer = SharedLogGuard;

    fn make_writer(&'writer self) -> Self::Writer {
        SharedLogGuard {
            output: Arc::clone(&self.output),
        }
    }
}

struct SharedLogGuard {
    output: Arc<std::sync::Mutex<Vec<u8>>>,
}

impl std::io::Write for SharedLogGuard {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn module_graph_emits_debug_logs() {
    let writer = SharedLogWriter::default();
    let subscriber = tracing_subscriber::registry().with(
        tracing_subscriber::fmt::layer()
            .with_writer(writer.clone())
            .with_ansi(false)
            .with_target(false)
            .with_filter(tracing_subscriber::filter::LevelFilter::from_level(
                Level::DEBUG,
            )),
    );
    let database = ModuleBuilder::new("DatabaseModule")
        .provider("DatabasePool")
        .export("DatabasePool")
        .build();
    let users = ModuleBuilder::new("UsersModule")
        .import("DatabaseModule")
        .provider("UsersService")
        .build();

    tracing::subscriber::with_default(subscriber, || {
        for _ in 0..16 {
            writer.clear();
            tracing_core::callsite::rebuild_interest_cache();
            ModuleGraph::from_modules([database.clone(), users.clone()]).unwrap();
            let logs = writer.contents();
            if logs.contains("validating module graph")
                && logs.contains("module graph node")
                && logs.contains("UsersModule")
                && logs.contains("module graph validated")
            {
                return;
            }
            std::thread::yield_now();
        }
    });

    let logs = writer.contents();
    assert!(logs.contains("validating module graph"));
    assert!(logs.contains("module graph node"));
    assert!(logs.contains("UsersModule"));
    assert!(logs.contains("module graph validated"));
}
