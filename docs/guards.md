# Guards

Guards are typed authorization checks.

```rust
#[async_trait::async_trait]
impl Guard<AppState> for AuthGuard {
    async fn check(&self, ctx: GuardContext<AppState>) -> Result<(), GuardError> {
        Ok(())
    }
}
```

Guard errors carry a typed `http::StatusCode` and a reason that can be mapped
into framework responses.
