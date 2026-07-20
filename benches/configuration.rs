use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};
use nidus_config::Config;
use serde::Deserialize;

const SERVICE_COUNT: usize = 128;

#[derive(Deserialize)]
struct BenchmarkConfig {
    services: Vec<ServiceConfig>,
}

impl BenchmarkConfig {
    fn checksum(&self) -> u64 {
        self.services.iter().map(ServiceConfig::checksum).sum()
    }
}

#[derive(Deserialize)]
struct ServiceConfig {
    port: u16,
    pool_size: u16,
    retry_limit: u8,
    timeout_ms: u64,
    enabled: bool,
    tls: bool,
    weight: u32,
    shard: u16,
}

impl ServiceConfig {
    fn checksum(&self) -> u64 {
        u64::from(self.port)
            + u64::from(self.pool_size)
            + u64::from(self.retry_limit)
            + self.timeout_ms
            + u64::from(self.enabled)
            + u64::from(self.tls)
            + u64::from(self.weight)
            + u64::from(self.shard)
    }
}

fn config_fixture() -> Config {
    let services = (0..SERVICE_COUNT)
        .map(|index| {
            serde_json::json!({
                "port": 3_000 + index,
                "pool_size": 16,
                "retry_limit": 5,
                "timeout_ms": 2_500,
                "enabled": true,
                "tls": true,
                "weight": 100,
                "shard": index,
            })
        })
        .collect::<Vec<_>>();

    Config::from_value(serde_json::json!({ "services": services })).unwrap()
}

fn configuration(c: &mut Criterion) {
    let config = config_fixture();

    c.bench_function("nidus config deserialize 128 services", |b| {
        b.iter(|| {
            let settings = config.deserialize::<BenchmarkConfig>().unwrap();
            black_box(settings.checksum());
            black_box(settings)
        });
    });
}

criterion_group!(benches, configuration);
criterion_main!(benches);
