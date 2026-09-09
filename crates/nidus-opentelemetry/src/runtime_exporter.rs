//! Supplies the application's Tokio context while the SDK batch thread polls
//! an asynchronous exporter. The guard is held for a single poll, never await.

use opentelemetry_sdk::{
    Resource,
    error::OTelSdkResult,
    trace::{SpanData, SpanExporter},
};
use std::{
    future::{Future, poll_fn},
    time::Duration,
};
use tokio::runtime::Handle;

#[derive(Debug)]
pub(crate) struct RuntimeExporter<E> {
    exporter: E,
    runtime: Option<Handle>,
}
impl<E> RuntimeExporter<E> {
    pub(crate) fn new(exporter: E) -> Self {
        Self {
            exporter,
            runtime: Handle::try_current().ok(),
        }
    }
}
impl<E: SpanExporter> SpanExporter for RuntimeExporter<E> {
    async fn export(&self, batch: Vec<SpanData>) -> OTelSdkResult {
        let future = {
            let _entered = self.runtime.as_ref().map(Handle::enter);
            self.exporter.export(batch)
        };
        let mut future = Box::pin(future);
        poll_fn(|cx| {
            let _entered = self.runtime.as_ref().map(Handle::enter);
            future.as_mut().poll(cx)
        })
        .await
    }
    fn shutdown_with_timeout(&self, timeout: Duration) -> OTelSdkResult {
        self.exporter.shutdown_with_timeout(timeout)
    }
    fn force_flush(&self) -> OTelSdkResult {
        self.exporter.force_flush()
    }
    fn set_resource(&mut self, resource: &Resource) {
        self.exporter.set_resource(resource);
    }
}
