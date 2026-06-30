#![cfg(feature = "sqlite")]

#[test]
fn sqlx_providers_do_not_support_sync_typed_registration() {
    let tests = trybuild::TestCases::new();
    tests.compile_fail("tests/ui/sqlx_provider_typed_registration.rs");
}
