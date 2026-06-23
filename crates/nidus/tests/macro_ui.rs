#[test]
fn public_macros_report_useful_compile_errors() {
    let tests = trybuild::TestCases::new();
    tests.pass("tests/ui/controller_valid.rs");
    tests.compile_fail("tests/ui/controller_missing_path.rs");
    tests.compile_fail("tests/ui/route_missing_path.rs");
}
