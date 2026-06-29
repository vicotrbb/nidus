#![no_main]

use libfuzzer_sys::fuzz_target;
use nidus_openapi::OpenApiRoute;

fuzz_target!(|data: &[u8]| {
    let path = String::from_utf8_lossy(data);
    let _ = OpenApiRoute::try_get(path.as_ref());
    let _ = OpenApiRoute::try_post(path.as_ref());
    let _ = OpenApiRoute::try_delete(path.as_ref());
});
