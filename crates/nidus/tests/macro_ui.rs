#[test]
fn public_macros_report_useful_compile_errors() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/controller_valid.rs");
    tests.pass("tests/ui/injectable_registers_provider.rs");
    tests.pass("tests/ui/module_generates_definition.rs");
    tests.pass("tests/ui/routes_generate_metadata.rs");
    tests.compile_fail("tests/ui/controller_missing_path.rs");
    tests.compile_fail("tests/ui/guard_missing_type.rs");
    tests.compile_fail("tests/ui/openapi_missing_summary.rs");
    tests.compile_fail("tests/ui/pipe_missing_type.rs");
    tests.compile_fail("tests/ui/route_missing_path.rs");
}
