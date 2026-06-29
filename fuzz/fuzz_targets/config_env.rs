#![no_main]

use libfuzzer_sys::fuzz_target;
use nidus_config::Config;

fuzz_target!(|data: &[u8]| {
    let input = String::from_utf8_lossy(data);
    let vars = input
        .split('\n')
        .enumerate()
        .map(|(index, value)| (format!("APP_FUZZ__KEY_{index}"), value.to_owned()));

    let config = Config::from_prefixed_vars("APP", vars);
    let _ = config.get_path(["fuzz"]);

    if input.trim_start().starts_with('{') {
        let _ = Config::from_json_str(&input);
    }
});
