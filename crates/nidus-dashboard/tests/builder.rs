use nidus_dashboard::{
    DashboardAuth, DashboardCapture, DashboardRetention, DashboardStorage, NidusDashboard,
};

#[test]
fn builder_fails_closed_without_auth() {
    let error = NidusDashboard::builder()
        .storage(DashboardStorage::memory())
        .build()
        .expect_err("dashboard must require auth by default");

    assert!(
        error
            .to_string()
            .contains("dashboard authentication is required"),
        "{error}"
    );
}

#[test]
fn builder_accepts_auth_storage_capture_and_retention() {
    let dashboard = NidusDashboard::builder()
        .path("/nidus/dashboard")
        .auth(DashboardAuth::bearer_token("dev-token"))
        .storage(DashboardStorage::memory())
        .capture(DashboardCapture::metadata_only())
        .retention(DashboardRetention::days(7).max_events(100_000))
        .build()
        .expect("authenticated dashboard should build");

    assert_eq!(dashboard.path(), "/nidus/dashboard");
}
