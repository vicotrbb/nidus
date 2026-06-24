use nidus::prelude::{Json, controller, get, openapi, routes};

#[derive(serde::Serialize, utoipa::ToSchema)]
pub struct HealthDto {
    status: &'static str,
}

#[controller("/health")]
pub struct HealthController;

#[routes]
impl HealthController {
    #[get("/")]
    #[openapi(
        summary = "Health check",
        tags = ["system"],
        status = 200,
        response = HealthDto
    )]
    pub async fn health(&self) -> Json<HealthDto> {
        Json(HealthDto { status: "ok" })
    }
}
