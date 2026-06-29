use std::{future::Future, pin::Pin, time::Duration};

use nidus::prelude::*;
use nidus_cache::{CacheConfig, MokaCacheProvider};
use nidus_sqlx::SqlitePoolProvider;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
struct AppConfig {
    bind_addr: String,
    database_url: String,
}

impl AppConfig {
    fn from_env() -> Self {
        Self {
            bind_addr: std::env::var("NIDUS_ADDR").unwrap_or_else(|_| "127.0.0.1:4302".to_owned()),
            database_url: std::env::var("COMMERCE_DATABASE_URL")
                .unwrap_or_else(|_| "sqlite::memory:".to_owned()),
        }
    }
}

#[nidus::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env();
    let address = config.bind_addr.clone();
    build_app(config).await?.listen(address).await?;
    Ok(())
}

async fn build_app(config: AppConfig) -> Result<HttpApplication> {
    let observability = Observability::production("external-commerce")
        .version(env!("CARGO_PKG_VERSION"))
        .environment("example")
        .prometheus()
        .tracing();
    let health = HealthRegistry::new()
        .live_check_sync("process", HealthStatus::up)
        .ready_check("sqlite", || async { HealthStatus::up() })
        .ready_check_sync("cache", HealthStatus::up);

    Nidus::create::<AppModule>()
        .with_singleton(config)?
        .with_singleton(observability.clone())?
        .build()
        .await
        .map(|app| {
            app.map_router(|router| {
                ApiDefaults::production("external-commerce")
                    .observability(&observability)
                    .health(health)
                    .request_ids(RequestIdConfig::development())
                    .apply(router.merge(observability.routes()))
            })
        })
}

struct InfrastructureModule;

impl Module for InfrastructureModule {
    fn definition() -> ModuleDefinition {
        ModuleBuilder::new("InfrastructureModule")
            .provider("SqlitePoolProvider")
            .provider("MokaCacheProvider")
            .export_typed::<SqlitePoolProvider>()
            .export_typed::<MokaCacheProvider>()
            .async_initializer(initialize_infrastructure)
            .build()
    }
}

fn initialize_infrastructure(
    container: &mut Container,
) -> Pin<Box<dyn Future<Output = Result<()>> + Send + '_>> {
    Box::pin(async move {
        let config = container.resolve::<AppConfig>()?;
        let observability = container.resolve::<Observability>()?;
        SqlitePoolProvider::builder()
            .database_url(&config.database_url)
            .max_connections(1)
            .observability(observability.adapter_observer())
            .register(container)
            .await
            .map_err(|error| NidusError::ApplicationBuild {
                message: error.to_string(),
            })?;
        MokaCacheProvider::builder()
            .config(
                CacheConfig::new()
                    .namespace("commerce")
                    .max_capacity(1_000)
                    .time_to_live(Duration::from_secs(60)),
            )
            .observability(observability.adapter_observer())
            .register(container)
            .map_err(|error| NidusError::ApplicationBuild {
                message: error.to_string(),
            })?;

        let database = container.resolve::<SqlitePoolProvider>()?;
        initialize_schema(database.pool()).await?;
        seed_products(database.pool()).await?;
        Ok(())
    })
}

async fn initialize_schema(pool: &sqlx::SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS products (
            id INTEGER PRIMARY KEY,
            sku TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            price_cents INTEGER NOT NULL,
            inventory INTEGER NOT NULL
        );
        CREATE TABLE IF NOT EXISTS carts (
            id TEXT PRIMARY KEY
        );
        CREATE TABLE IF NOT EXISTS cart_items (
            cart_id TEXT NOT NULL,
            product_id INTEGER NOT NULL,
            quantity INTEGER NOT NULL,
            PRIMARY KEY (cart_id, product_id)
        );
        CREATE TABLE IF NOT EXISTS orders (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            idempotency_key TEXT NOT NULL UNIQUE,
            cart_id TEXT NOT NULL,
            total_cents INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await
    .map_err(db_error)?;
    Ok(())
}

async fn seed_products(pool: &sqlx::SqlitePool) -> Result<()> {
    let (count,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM products")
        .fetch_one(pool)
        .await
        .map_err(db_error)?;
    if count == 0 {
        for (id, sku, name, price_cents, inventory) in [
            (1_i64, "tee-basic", "Basic Tee", 2500_i64, 12_i64),
            (2_i64, "mug-stone", "Stone Mug", 1800_i64, 8_i64),
        ] {
            sqlx::query(
                "INSERT INTO products (id, sku, name, price_cents, inventory) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(sku)
            .bind(name)
            .bind(price_cents)
            .bind(inventory)
            .execute(pool)
            .await
            .map_err(db_error)?;
        }
    }
    Ok(())
}

fn db_error(error: sqlx::Error) -> NidusError {
    NidusError::ApplicationBuild {
        message: error.to_string(),
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ProductDto {
    id: i64,
    sku: String,
    name: String,
    price_cents: i64,
    inventory: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CartDto {
    id: String,
    items: Vec<CartItemDto>,
    total_cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct CartItemDto {
    product_id: i64,
    sku: String,
    name: String,
    quantity: i64,
    line_total_cents: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct OrderDto {
    id: i64,
    cart_id: String,
    total_cents: i64,
    request_id: String,
}

#[derive(Debug, Deserialize)]
struct CreateCart {
    id: String,
}

#[derive(Debug, Deserialize)]
struct AddCartItem {
    product_id: i64,
    quantity: i64,
}

#[injectable]
struct ProductRepository {
    database: Inject<SqlitePoolProvider>,
}

impl ProductRepository {
    async fn list(&self) -> std::result::Result<Vec<ProductDto>, HttpError> {
        sqlx::query_as::<_, (i64, String, String, i64, i64)>(
            "SELECT id, sku, name, price_cents, inventory FROM products ORDER BY id",
        )
        .fetch_all(self.database.pool())
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|(id, sku, name, price_cents, inventory)| ProductDto {
                    id,
                    sku,
                    name,
                    price_cents,
                    inventory,
                })
                .collect()
        })
        .map_err(|_| HttpError::internal_server_error())
    }
}

#[injectable]
struct CartRepository {
    database: Inject<SqlitePoolProvider>,
}

impl CartRepository {
    async fn create_cart(&self, id: &str) -> std::result::Result<CartDto, HttpError> {
        validate_text("cart id", id)?;
        sqlx::query("INSERT OR IGNORE INTO carts (id) VALUES (?)")
            .bind(id)
            .execute(self.database.pool())
            .await
            .map_err(|_| HttpError::internal_server_error())?;
        self.get_cart(id).await
    }

    async fn get_cart(&self, id: &str) -> std::result::Result<CartDto, HttpError> {
        let exists = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM carts WHERE id = ?")
            .bind(id)
            .fetch_one(self.database.pool())
            .await
            .map_err(|_| HttpError::internal_server_error())?
            .0;
        if exists == 0 {
            return Err(HttpError::not_found(format!("cart {id} was not found")));
        }

        let items = sqlx::query_as::<_, (i64, String, String, i64, i64)>(
            r#"
            SELECT p.id, p.sku, p.name, ci.quantity, p.price_cents * ci.quantity
            FROM cart_items ci
            JOIN products p ON p.id = ci.product_id
            WHERE ci.cart_id = ?
            ORDER BY p.id
            "#,
        )
        .bind(id)
        .fetch_all(self.database.pool())
        .await
        .map_err(|_| HttpError::internal_server_error())?
        .into_iter()
        .map(
            |(product_id, sku, name, quantity, line_total_cents)| CartItemDto {
                product_id,
                sku,
                name,
                quantity,
                line_total_cents,
            },
        )
        .collect::<Vec<_>>();
        let total_cents = items.iter().map(|item| item.line_total_cents).sum();
        Ok(CartDto {
            id: id.to_owned(),
            items,
            total_cents,
        })
    }

    async fn add_item(
        &self,
        cart_id: &str,
        input: AddCartItem,
    ) -> std::result::Result<CartDto, HttpError> {
        if input.quantity <= 0 {
            return Err(HttpError::bad_request("quantity must be positive"));
        }
        self.get_cart(cart_id).await?;
        let product = sqlx::query_as::<_, (i64,)>("SELECT inventory FROM products WHERE id = ?")
            .bind(input.product_id)
            .fetch_optional(self.database.pool())
            .await
            .map_err(|_| HttpError::internal_server_error())?
            .ok_or_else(|| HttpError::not_found("product was not found"))?;
        if product.0 < input.quantity {
            return Err(HttpError::conflict("not enough inventory"));
        }
        sqlx::query(
            r#"
            INSERT INTO cart_items (cart_id, product_id, quantity)
            VALUES (?, ?, ?)
            ON CONFLICT(cart_id, product_id)
            DO UPDATE SET quantity = quantity + excluded.quantity
            "#,
        )
        .bind(cart_id)
        .bind(input.product_id)
        .bind(input.quantity)
        .execute(self.database.pool())
        .await
        .map_err(|_| HttpError::internal_server_error())?;
        self.get_cart(cart_id).await
    }

    async fn checkout(
        &self,
        cart_id: &str,
        idempotency_key: &str,
    ) -> std::result::Result<OrderDto, HttpError> {
        validate_text("idempotency key", idempotency_key)?;
        if let Some(existing) = sqlx::query_as::<_, (i64, String, i64)>(
            "SELECT id, cart_id, total_cents FROM orders WHERE idempotency_key = ?",
        )
        .bind(idempotency_key)
        .fetch_optional(self.database.pool())
        .await
        .map_err(|_| HttpError::internal_server_error())?
        {
            return Ok(OrderDto {
                id: existing.0,
                cart_id: existing.1,
                total_cents: existing.2,
                request_id: String::new(),
            });
        }

        let cart = self.get_cart(cart_id).await?;
        if cart.items.is_empty() {
            return Err(HttpError::bad_request("cart is empty"));
        }
        for item in &cart.items {
            let inventory =
                sqlx::query_as::<_, (i64,)>("SELECT inventory FROM products WHERE id = ?")
                    .bind(item.product_id)
                    .fetch_one(self.database.pool())
                    .await
                    .map_err(|_| HttpError::internal_server_error())?
                    .0;
            if inventory < item.quantity {
                return Err(HttpError::conflict(format!(
                    "not enough inventory for {}",
                    item.sku
                )));
            }
        }
        for item in &cart.items {
            sqlx::query("UPDATE products SET inventory = inventory - ? WHERE id = ?")
                .bind(item.quantity)
                .bind(item.product_id)
                .execute(self.database.pool())
                .await
                .map_err(|_| HttpError::internal_server_error())?;
        }
        let result = sqlx::query(
            "INSERT INTO orders (idempotency_key, cart_id, total_cents) VALUES (?, ?, ?)",
        )
        .bind(idempotency_key)
        .bind(cart_id)
        .bind(cart.total_cents)
        .execute(self.database.pool())
        .await
        .map_err(|_| HttpError::internal_server_error())?;
        sqlx::query("DELETE FROM cart_items WHERE cart_id = ?")
            .bind(cart_id)
            .execute(self.database.pool())
            .await
            .map_err(|_| HttpError::internal_server_error())?;
        Ok(OrderDto {
            id: result.last_insert_rowid(),
            cart_id: cart_id.to_owned(),
            total_cents: cart.total_cents,
            request_id: String::new(),
        })
    }
}

#[injectable]
struct CommerceService {
    products: Inject<ProductRepository>,
    carts: Inject<CartRepository>,
    cache: Optional<MokaCacheProvider>,
}

impl CommerceService {
    async fn list_products(&self) -> std::result::Result<Vec<ProductDto>, HttpError> {
        if let Some(cache) = self.cache.as_ref()
            && let Some(bytes) = cache.get("products").await
        {
            return serde_json::from_slice(&bytes).map_err(|_| HttpError::internal_server_error());
        }
        let products = self.products.list().await?;
        if let Some(cache) = self.cache.as_ref() {
            let bytes =
                serde_json::to_vec(&products).map_err(|_| HttpError::internal_server_error())?;
            cache.insert("products", bytes).await;
        }
        Ok(products)
    }

    async fn create_cart(&self, input: CreateCart) -> std::result::Result<CartDto, HttpError> {
        self.carts.create_cart(&input.id).await
    }

    async fn get_cart(&self, id: &str) -> std::result::Result<CartDto, HttpError> {
        self.carts.get_cart(id).await
    }

    async fn add_item(
        &self,
        cart_id: &str,
        input: AddCartItem,
    ) -> std::result::Result<CartDto, HttpError> {
        self.carts.add_item(cart_id, input).await
    }

    async fn checkout(
        &self,
        cart_id: &str,
        idempotency_key: &str,
    ) -> std::result::Result<OrderDto, HttpError> {
        let order = self.carts.checkout(cart_id, idempotency_key).await?;
        if let Some(cache) = self.cache.as_ref() {
            cache.invalidate("products").await;
        }
        Ok(order)
    }
}

fn validate_text(field: &str, value: &str) -> std::result::Result<(), HttpError> {
    if value.trim().is_empty() {
        Err(HttpError::bad_request(format!("{field} must not be blank")))
    } else {
        Ok(())
    }
}

#[controller("")]
struct CommerceController {
    service: Inject<CommerceService>,
}

#[routes]
impl CommerceController {
    #[get("/products")]
    async fn products(&self) -> std::result::Result<Json<Vec<ProductDto>>, HttpError> {
        Ok(Json(self.service.list_products().await?))
    }

    #[post("/carts")]
    async fn create_cart(
        &self,
        Json(input): Json<CreateCart>,
    ) -> std::result::Result<(StatusCode, Json<CartDto>), HttpError> {
        Ok((
            StatusCode::CREATED,
            Json(self.service.create_cart(input).await?),
        ))
    }

    #[get("/carts/:id")]
    async fn get_cart(
        &self,
        Path(id): Path<String>,
    ) -> std::result::Result<Json<CartDto>, HttpError> {
        Ok(Json(self.service.get_cart(&id).await?))
    }

    #[post("/carts/:id/items")]
    async fn add_item(
        &self,
        Path(id): Path<String>,
        Json(input): Json<AddCartItem>,
    ) -> std::result::Result<Json<CartDto>, HttpError> {
        Ok(Json(self.service.add_item(&id, input).await?))
    }

    #[post("/carts/:id/checkout")]
    async fn checkout(
        &self,
        headers: HeaderMap,
        context: RequestContext,
        Path(id): Path<String>,
    ) -> std::result::Result<Json<OrderDto>, HttpError> {
        let idempotency_key = headers
            .get("idempotency-key")
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| HttpError::bad_request("idempotency-key header is required"))?;
        let mut order = self.service.checkout(&id, idempotency_key).await?;
        order.request_id = context.request_id().to_owned();
        Ok(Json(order))
    }
}

#[module(
    imports(InfrastructureModule),
    providers(ProductRepository, CartRepository, CommerceService),
    controllers(CommerceController)
)]
struct AppModule;

#[cfg(test)]
mod tests {
    use super::*;
    use nidus_testing::TestApp;
    use serde_json::{Value, json};

    async fn app() -> TestApp {
        TestApp::from_router(
            build_app(AppConfig {
                bind_addr: "127.0.0.1:0".to_owned(),
                database_url: "sqlite::memory:".to_owned(),
            })
            .await
            .unwrap()
            .into_router(),
        )
    }

    #[tokio::test]
    async fn product_cart_and_idempotent_checkout_flow_uses_database_and_cache() {
        let app = app().await;
        let products = app.get("/products").send().await;
        products.assert_status(StatusCode::OK);
        assert_eq!(products.json::<Value>()[0]["inventory"], 12);

        app.post("/carts")
            .json(&json!({ "id": "cart-1" }))
            .send()
            .await
            .assert_status(StatusCode::CREATED);

        let cart = app
            .post("/carts/cart-1/items")
            .json(&json!({ "product_id": 1, "quantity": 2 }))
            .send()
            .await;
        cart.assert_status(StatusCode::OK);
        assert_eq!(cart.json::<Value>()["total_cents"], 5000);

        let checked_out = app
            .post("/carts/cart-1/checkout")
            .header("idempotency-key", "checkout-1")
            .header("x-request-id", "018f4ad7-56ce-4f6a-a759-29f4438d8d88")
            .send()
            .await;
        checked_out.assert_status(StatusCode::OK);
        let first_order = checked_out.json::<Value>();
        assert_eq!(first_order["total_cents"], 5000);
        assert_eq!(
            first_order["request_id"],
            "018f4ad7-56ce-4f6a-a759-29f4438d8d88"
        );

        let repeated = app
            .post("/carts/cart-1/checkout")
            .header("idempotency-key", "checkout-1")
            .send()
            .await;
        repeated.assert_status(StatusCode::OK);
        assert_eq!(repeated.json::<Value>()["id"], first_order["id"]);

        let updated_products = app.get("/products").send().await;
        updated_products.assert_status(StatusCode::OK);
        assert_eq!(updated_products.json::<Value>()[0]["inventory"], 10);
    }

    #[tokio::test]
    async fn validation_not_found_and_health_are_wired() {
        let app = app().await;
        app.get("/health/live")
            .send()
            .await
            .assert_status(StatusCode::OK);
        app.get("/health/ready")
            .send()
            .await
            .assert_status(StatusCode::OK);
        app.post("/carts")
            .json(&json!({ "id": "" }))
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);
        app.post("/carts/missing/items")
            .json(&json!({ "product_id": 1, "quantity": 1 }))
            .send()
            .await
            .assert_status(StatusCode::NOT_FOUND);
        app.post("/carts/missing/checkout")
            .send()
            .await
            .assert_status(StatusCode::BAD_REQUEST);
    }
}
