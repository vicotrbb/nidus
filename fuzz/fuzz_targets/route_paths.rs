#![no_main]

use libfuzzer_sys::fuzz_target;
use nidus_http::router::RouteDefinition;

async fn ok() -> &'static str {
    "ok"
}

fuzz_target!(|data: &[u8]| {
    let path = String::from_utf8_lossy(data);
    let _ = RouteDefinition::try_get(path.as_ref(), ok);
    let _ = RouteDefinition::try_post(path.as_ref(), ok);
});
