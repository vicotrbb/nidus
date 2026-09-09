use nidus_opentelemetry::{OpenTelemetryConfig, OpenTelemetryPipeline};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing_subscriber::prelude::*;

#[tokio::test]
async fn real_http_export_flushes_a_nonempty_batch_on_the_sdk_thread() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut data = Vec::new();
        let header_end = loop {
            let mut byte = [0];
            stream.read_exact(&mut byte).await.unwrap();
            data.push(byte[0]);
            if data.ends_with(b"\r\n\r\n") {
                break data.len();
            }
        };
        let headers = String::from_utf8_lossy(&data).to_lowercase();
        assert!(headers.starts_with("post /v1/traces "));
        let length: usize = headers
            .lines()
            .find_map(|line| line.strip_prefix("content-length: "))
            .unwrap()
            .parse()
            .unwrap();
        assert!(length > 0);
        data.resize(header_end + length, 0);
        stream.read_exact(&mut data[header_end..]).await.unwrap();
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();
        length
    });
    let config =
        OpenTelemetryConfig::http_protobuf("regression", format!("http://{address}/v1/traces"))
            .allow_insecure_local_endpoint()
            .unwrap();
    let pipeline = OpenTelemetryPipeline::init(config).unwrap();
    let subscriber = tracing_subscriber::registry().with(pipeline.tracing_layer());
    {
        let _default = tracing::subscriber::set_default(subscriber);
        let span = tracing::info_span!("real.http.export");
        let _entered = span.enter();
        tracing::info!("must reach loopback collector");
    }
    let flushed = pipeline.force_flush().await;
    let stopped = pipeline.shutdown().await;
    if flushed.is_err() {
        server.abort();
        let _ = server.await;
    } else {
        assert!(server.await.unwrap() > 0);
    }
    flushed.unwrap();
    stopped.unwrap();
}
