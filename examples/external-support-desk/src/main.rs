use std::{
    collections::BTreeMap,
    sync::{Arc, RwLock},
};

use nidus::prelude::*;
use serde::{Deserialize, Serialize};

const API_KEY: &str = "support-secret";

#[nidus::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let address = std::env::var("NIDUS_ADDR").unwrap_or_else(|_| "127.0.0.1:4301".to_owned());
    build_app().await?.listen(address).await?;
    Ok(())
}

async fn build_app() -> Result<HttpApplication> {
    Nidus::create::<AppModule>().build().await.map(|app| {
        app.map_router(|router| {
            ApiDefaults::production("external-support-desk")
                .request_ids(RequestIdConfig::development())
                .apply(router)
        })
    })
}

fn require_api_key(headers: &HeaderMap) -> std::result::Result<(), HttpError> {
    match headers
        .get("x-api-key")
        .and_then(|value| value.to_str().ok())
    {
        Some(API_KEY) => Ok(()),
        _ => Err(HttpError::unauthorized("missing or invalid x-api-key")),
    }
}

#[derive(Clone, Default)]
struct TicketStore {
    state: Arc<RwLock<TicketState>>,
}

impl ProviderRegistrant for TicketStore {
    fn register_provider(container: &mut Container) -> Result<()> {
        container.register_singleton(Self::default())
    }
}

#[derive(Default)]
struct TicketState {
    next_ticket_id: i64,
    next_comment_id: i64,
    tickets: BTreeMap<i64, Ticket>,
}

#[derive(Clone, Debug)]
struct Ticket {
    id: i64,
    subject: String,
    description: String,
    priority: Priority,
    status: TicketStatus,
    assignee: Option<String>,
    comments: Vec<Comment>,
}

#[derive(Clone, Debug)]
struct Comment {
    id: i64,
    author: String,
    body: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Priority {
    Low,
    Normal,
    High,
    Urgent,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum TicketStatus {
    Open,
    Assigned,
    Closed,
}

#[derive(Debug, Deserialize)]
struct CreateTicket {
    subject: String,
    description: String,
    priority: Priority,
}

#[derive(Debug, Deserialize)]
struct AddComment {
    author: String,
    body: String,
}

#[derive(Debug, Deserialize)]
struct AssignTicket {
    assignee: String,
}

#[derive(Clone, Debug, Serialize)]
struct TicketDto {
    id: i64,
    subject: String,
    description: String,
    priority: Priority,
    status: TicketStatus,
    assignee: Option<String>,
    comments: Vec<CommentDto>,
    request_id: String,
}

#[derive(Clone, Debug, Serialize)]
struct CommentDto {
    id: i64,
    author: String,
    body: String,
}

#[injectable]
struct TicketRepository {
    store: Inject<TicketStore>,
}

impl TicketRepository {
    fn create(&self, input: CreateTicket) -> Ticket {
        let mut state = self
            .store
            .state
            .write()
            .expect("ticket store lock poisoned");
        state.next_ticket_id += 1;
        let ticket = Ticket {
            id: state.next_ticket_id,
            subject: input.subject,
            description: input.description,
            priority: input.priority,
            status: TicketStatus::Open,
            assignee: None,
            comments: Vec::new(),
        };
        state.tickets.insert(ticket.id, ticket.clone());
        ticket
    }

    fn find(&self, id: i64) -> Option<Ticket> {
        self.store
            .state
            .read()
            .expect("ticket store lock poisoned")
            .tickets
            .get(&id)
            .cloned()
    }

    fn list(&self) -> Vec<Ticket> {
        self.store
            .state
            .read()
            .expect("ticket store lock poisoned")
            .tickets
            .values()
            .cloned()
            .collect()
    }

    fn comment(&self, id: i64, input: AddComment) -> Option<Ticket> {
        let mut state = self
            .store
            .state
            .write()
            .expect("ticket store lock poisoned");
        state.next_comment_id += 1;
        let comment_id = state.next_comment_id;
        let ticket = state.tickets.get_mut(&id)?;
        ticket.comments.push(Comment {
            id: comment_id,
            author: input.author,
            body: input.body,
        });
        Some(ticket.clone())
    }

    fn assign(&self, id: i64, input: AssignTicket) -> Option<Ticket> {
        let mut state = self
            .store
            .state
            .write()
            .expect("ticket store lock poisoned");
        let ticket = state.tickets.get_mut(&id)?;
        ticket.assignee = Some(input.assignee);
        ticket.status = TicketStatus::Assigned;
        Some(ticket.clone())
    }

    fn close(&self, id: i64) -> Option<Ticket> {
        let mut state = self
            .store
            .state
            .write()
            .expect("ticket store lock poisoned");
        let ticket = state.tickets.get_mut(&id)?;
        ticket.status = TicketStatus::Closed;
        Some(ticket.clone())
    }
}

#[injectable]
struct TicketService {
    repository: Inject<TicketRepository>,
}

impl TicketService {
    fn create(&self, input: CreateTicket) -> std::result::Result<Ticket, HttpError> {
        validate_text("subject", &input.subject)?;
        validate_text("description", &input.description)?;
        Ok(self.repository.create(input))
    }

    fn list(&self) -> Vec<Ticket> {
        self.repository.list()
    }

    fn find(&self, id: i64) -> std::result::Result<Ticket, HttpError> {
        self.repository
            .find(id)
            .ok_or_else(|| HttpError::not_found(format!("ticket {id} was not found")))
    }

    fn comment(&self, id: i64, input: AddComment) -> std::result::Result<Ticket, HttpError> {
        validate_text("author", &input.author)?;
        validate_text("body", &input.body)?;
        self.repository
            .comment(id, input)
            .ok_or_else(|| HttpError::not_found(format!("ticket {id} was not found")))
    }

    fn assign(&self, id: i64, input: AssignTicket) -> std::result::Result<Ticket, HttpError> {
        validate_text("assignee", &input.assignee)?;
        self.repository
            .assign(id, input)
            .ok_or_else(|| HttpError::not_found(format!("ticket {id} was not found")))
    }

    fn close(&self, id: i64) -> std::result::Result<Ticket, HttpError> {
        let ticket = self.find(id)?;
        if ticket.status == TicketStatus::Closed {
            return Err(HttpError::conflict("ticket is already closed"));
        }
        self.repository
            .close(id)
            .ok_or_else(|| HttpError::not_found(format!("ticket {id} was not found")))
    }
}

fn validate_text(field: &str, value: &str) -> std::result::Result<(), HttpError> {
    if value.trim().is_empty() {
        Err(HttpError::bad_request(format!("{field} must not be blank")))
    } else {
        Ok(())
    }
}

#[controller("/tickets")]
struct TicketsController {
    service: Inject<TicketService>,
}

#[routes]
impl TicketsController {
    #[post("/")]
    async fn create(
        &self,
        headers: HeaderMap,
        context: RequestContext,
        Json(input): Json<CreateTicket>,
    ) -> std::result::Result<(StatusCode, Json<TicketDto>), HttpError> {
        require_api_key(&headers)?;
        let ticket = self.service.create(input)?;
        Ok((StatusCode::CREATED, Json(to_dto(ticket, &context))))
    }

    #[get("/")]
    async fn list(
        &self,
        headers: HeaderMap,
        context: RequestContext,
    ) -> std::result::Result<Json<Vec<TicketDto>>, HttpError> {
        require_api_key(&headers)?;
        Ok(Json(
            self.service
                .list()
                .into_iter()
                .map(|ticket| to_dto(ticket, &context))
                .collect(),
        ))
    }

    #[get("/:id")]
    async fn find(
        &self,
        headers: HeaderMap,
        context: RequestContext,
        Path(id): Path<i64>,
    ) -> std::result::Result<Json<TicketDto>, HttpError> {
        require_api_key(&headers)?;
        Ok(Json(to_dto(self.service.find(id)?, &context)))
    }

    #[post("/:id/comments")]
    async fn comment(
        &self,
        headers: HeaderMap,
        context: RequestContext,
        Path(id): Path<i64>,
        Json(input): Json<AddComment>,
    ) -> std::result::Result<Json<TicketDto>, HttpError> {
        require_api_key(&headers)?;
        Ok(Json(to_dto(self.service.comment(id, input)?, &context)))
    }

    #[post("/:id/assign")]
    async fn assign(
        &self,
        headers: HeaderMap,
        context: RequestContext,
        Path(id): Path<i64>,
        Json(input): Json<AssignTicket>,
    ) -> std::result::Result<Json<TicketDto>, HttpError> {
        require_api_key(&headers)?;
        Ok(Json(to_dto(self.service.assign(id, input)?, &context)))
    }

    #[post("/:id/close")]
    async fn close(
        &self,
        headers: HeaderMap,
        context: RequestContext,
        Path(id): Path<i64>,
    ) -> std::result::Result<Json<TicketDto>, HttpError> {
        require_api_key(&headers)?;
        Ok(Json(to_dto(self.service.close(id)?, &context)))
    }
}

fn to_dto(ticket: Ticket, context: &RequestContext) -> TicketDto {
    TicketDto {
        id: ticket.id,
        subject: ticket.subject,
        description: ticket.description,
        priority: ticket.priority,
        status: ticket.status,
        assignee: ticket.assignee,
        comments: ticket
            .comments
            .into_iter()
            .map(|comment| CommentDto {
                id: comment.id,
                author: comment.author,
                body: comment.body,
            })
            .collect(),
        request_id: context.request_id().to_owned(),
    }
}

#[module(
    providers(TicketStore, TicketRepository, TicketService),
    controllers(TicketsController)
)]
struct AppModule;

#[cfg(test)]
mod tests {
    use super::*;
    use nidus_testing::TestApp;
    use serde_json::{Value, json};

    const REQUEST_ID: &str = "018f4ad7-56ce-4f6a-a759-29f4438d8d78";

    async fn app() -> TestApp {
        TestApp::from_router(build_app().await.unwrap().into_router())
    }

    fn authed(request: nidus_testing::TestRequest) -> nidus_testing::TestRequest {
        request
            .header("x-api-key", API_KEY)
            .header("x-request-id", REQUEST_ID)
    }

    #[tokio::test]
    async fn ticket_lifecycle_uses_injected_services() {
        let app = app().await;
        let created = authed(app.post("/tickets"))
            .json(&json!({
                "subject": "Cannot deploy",
                "description": "The deployment pipeline is blocked",
                "priority": "urgent"
            }))
            .send()
            .await;
        created.assert_status(StatusCode::CREATED);
        let body: Value = created.json();
        assert_eq!(body["id"], 1);
        assert_eq!(body["status"], "open");
        assert_eq!(body["request_id"], REQUEST_ID);

        let assigned = authed(app.post("/tickets/1/assign"))
            .json(&json!({ "assignee": "Ada" }))
            .send()
            .await;
        assigned.assert_status(StatusCode::OK);
        assert_eq!(assigned.json::<Value>()["status"], "assigned");

        let commented = authed(app.post("/tickets/1/comments"))
            .json(&json!({ "author": "Grace", "body": "Looking now" }))
            .send()
            .await;
        commented.assert_status(StatusCode::OK);
        assert_eq!(
            commented.json::<Value>()["comments"][0]["body"],
            "Looking now"
        );

        let closed = authed(app.post("/tickets/1/close")).send().await;
        closed.assert_status(StatusCode::OK);
        assert_eq!(closed.json::<Value>()["status"], "closed");
    }

    #[tokio::test]
    async fn auth_validation_and_not_found_are_stable() {
        let app = app().await;
        app.get("/tickets")
            .send()
            .await
            .assert_status(StatusCode::UNAUTHORIZED);

        authed(app.post("/tickets"))
            .json(&json!({
                "subject": "",
                "description": "missing subject",
                "priority": "normal"
            }))
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);

        authed(app.get("/tickets/404"))
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND);
    }
}
