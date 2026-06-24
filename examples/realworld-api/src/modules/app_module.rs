use nidus::prelude::module;

#[module]
pub struct AppModule {
    imports: (
        crate::modules::DatabaseModule,
        crate::modules::AuthModule,
        crate::modules::UsersModule,
        crate::modules::ProjectsModule,
    ),
    controllers: [crate::health::HealthController],
}
