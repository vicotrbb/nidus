use nidus::prelude::module;

#[module]
pub struct AuthModule {
    providers: (crate::auth::AuthService, crate::auth::guard::ApiKeyGuard),
    exports: (crate::auth::AuthService, crate::auth::guard::ApiKeyGuard),
}
