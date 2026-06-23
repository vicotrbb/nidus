//! Framework error types.

/// Convenient result type for Nidus operations.
pub type Result<T> = std::result::Result<T, NidusError>;

/// Errors emitted by Nidus core primitives.
#[derive(Debug, thiserror::Error)]
pub enum NidusError {
    /// A dependency was requested but no provider exists for its concrete type.
    #[error("missing provider for type `{type_name}`")]
    MissingProvider {
        /// Rust type name requested from the container.
        type_name: &'static str,
    },

    /// A provider was registered more than once for the same concrete type.
    #[error("duplicate provider for type `{type_name}`")]
    DuplicateProvider {
        /// Rust type name registered more than once.
        type_name: &'static str,
    },

    /// A registered provider factory returned an error.
    #[error("provider factory failed for type `{type_name}`: {source}")]
    ProviderFactory {
        /// Rust type name whose factory failed.
        type_name: &'static str,
        /// Underlying framework error.
        #[source]
        source: Box<NidusError>,
    },

    /// A module imports another module that is not present in the graph.
    #[error("module `{module}` imports missing module `{import}`")]
    MissingModuleImport {
        /// Module declaring the import.
        module: String,
        /// Missing imported module.
        import: String,
    },

    /// A circular module import chain was detected.
    #[error("circular module import detected: {}", cycle.join(" -> "))]
    CircularModuleImport {
        /// Ordered cycle path.
        cycle: Vec<String>,
    },
}
