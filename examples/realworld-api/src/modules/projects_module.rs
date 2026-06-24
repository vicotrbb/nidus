use nidus::prelude::module;

#[module]
pub struct ProjectsModule {
    providers: (
        crate::projects::ProjectsRepository,
        crate::projects::ProjectsService,
        crate::tasks::TasksRepository,
        crate::tasks::TasksService,
    ),
    controllers: (
        crate::projects::ProjectsController,
        crate::tasks::TasksController,
    ),
    exports: (crate::projects::ProjectsService, crate::tasks::TasksService),
}
