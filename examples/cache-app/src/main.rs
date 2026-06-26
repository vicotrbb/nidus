use nidus::prelude::{Container, Optional};
use nidus_cache::{CacheConfig, MokaCacheProvider};

struct UsersService {
    cache: Optional<MokaCacheProvider>,
}

impl UsersService {
    fn new(cache: Optional<MokaCacheProvider>) -> Self {
        Self { cache }
    }

    async fn cached_display_name(&self, user_id: &str) -> Option<String> {
        let cache = self.cache.as_ref()?;
        let bytes = cache.get(user_id).await?;
        String::from_utf8(bytes).ok()
    }
}

fn build_container() -> nidus::prelude::Result<Container> {
    let mut container = Container::new();
    MokaCacheProvider::builder()
        .config(CacheConfig::new().namespace("users").max_capacity(1_000))
        .register(&mut container)
        .map_err(|error| nidus::prelude::NidusError::ApplicationBuild {
            message: error.to_string(),
        })?;
    container.register_singleton_factory(|container| {
        Ok(UsersService::new(
            container.optional::<MokaCacheProvider>()?,
        ))
    })?;
    Ok(container)
}

#[tokio::main]
async fn main() -> nidus::prelude::Result<()> {
    let container = build_container()?;
    let cache = container.resolve::<MokaCacheProvider>()?;
    cache.insert("42", b"Ada".to_vec()).await;
    let _name = container
        .resolve::<UsersService>()?
        .cached_display_name("42")
        .await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn example_wires_cache_provider() {
        let container = build_container().unwrap();
        assert!(container.resolve::<MokaCacheProvider>().is_ok());
        assert!(container.resolve::<UsersService>().is_ok());
    }
}
