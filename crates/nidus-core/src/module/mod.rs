//! Module graph primitives.

use std::collections::{HashMap, HashSet};

use crate::{NidusError, Result};

/// A Rust type that describes a Nidus module.
pub trait Module {
    /// Returns this module's static definition.
    fn definition() -> ModuleDefinition;
}

/// Explicit module metadata used to validate application structure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleDefinition {
    name: String,
    imports: Vec<String>,
    providers: Vec<String>,
    controllers: Vec<String>,
    exports: Vec<String>,
}

impl ModuleDefinition {
    /// Returns the module name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns explicit imported modules.
    pub fn imports(&self) -> &[String] {
        &self.imports
    }

    /// Returns providers owned by the module.
    pub fn providers(&self) -> &[String] {
        &self.providers
    }

    /// Returns controllers owned by the module.
    pub fn controllers(&self) -> &[String] {
        &self.controllers
    }

    /// Returns providers exported to importing modules.
    pub fn exports(&self) -> &[String] {
        &self.exports
    }
}

/// Builder for explicit module definitions.
#[derive(Debug)]
pub struct ModuleBuilder {
    definition: ModuleDefinition,
}

impl ModuleBuilder {
    /// Starts a module definition.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            definition: ModuleDefinition {
                name: name.into(),
                imports: Vec::new(),
                providers: Vec::new(),
                controllers: Vec::new(),
                exports: Vec::new(),
            },
        }
    }

    /// Adds an explicit module import.
    pub fn import(mut self, name: impl Into<String>) -> Self {
        self.definition.imports.push(name.into());
        self
    }

    /// Adds a provider declaration.
    pub fn provider(mut self, name: impl Into<String>) -> Self {
        self.definition.providers.push(name.into());
        self
    }

    /// Adds a controller declaration.
    pub fn controller(mut self, name: impl Into<String>) -> Self {
        self.definition.controllers.push(name.into());
        self
    }

    /// Adds a provider export declaration.
    pub fn export(mut self, name: impl Into<String>) -> Self {
        self.definition.exports.push(name.into());
        self
    }

    /// Completes the module definition.
    pub fn build(self) -> ModuleDefinition {
        self.definition
    }
}

/// Validated graph of module definitions.
#[derive(Debug)]
pub struct ModuleGraph {
    modules: HashMap<String, ModuleDefinition>,
}

impl ModuleGraph {
    /// Builds and validates a module graph.
    pub fn from_modules(modules: impl IntoIterator<Item = ModuleDefinition>) -> Result<Self> {
        let mut registered = HashMap::new();
        for module in modules {
            let name = module.name.clone();
            if registered.insert(name.clone(), module).is_some() {
                return Err(NidusError::DuplicateModule { module: name });
            }
        }
        let graph = Self {
            modules: registered,
        };
        tracing::debug!(
            module_count = graph.modules.len(),
            "validating module graph"
        );
        for module in graph.modules.values() {
            tracing::debug!(
                module = %module.name,
                imports = ?module.imports,
                providers = ?module.providers,
                controllers = ?module.controllers,
                exports = ?module.exports,
                "module graph node"
            );
        }
        graph.validate_imports_exist()?;
        graph.validate_acyclic()?;
        graph.validate_exports_are_local()?;
        graph.validate_visible_providers_unambiguous()?;
        tracing::debug!(module_count = graph.modules.len(), "module graph validated");
        Ok(graph)
    }

    /// Returns a module definition by name.
    pub fn get(&self, name: &str) -> Option<&ModuleDefinition> {
        self.modules.get(name)
    }

    fn validate_imports_exist(&self) -> Result<()> {
        for module in self.modules.values() {
            for import in &module.imports {
                if !self.modules.contains_key(import) {
                    return Err(NidusError::MissingModuleImport {
                        module: module.name.clone(),
                        import: import.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_acyclic(&self) -> Result<()> {
        let mut visiting = HashSet::new();
        let mut visited = HashSet::new();
        let mut stack = Vec::new();

        for name in self.modules.keys() {
            self.visit(name, &mut visiting, &mut visited, &mut stack)?;
        }
        Ok(())
    }

    fn validate_exports_are_local(&self) -> Result<()> {
        for module in self.modules.values() {
            let providers = module.providers.iter().collect::<HashSet<_>>();
            for export in &module.exports {
                if !providers.contains(export) {
                    return Err(NidusError::MissingProviderExport {
                        module: module.name.clone(),
                        provider: export.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_visible_providers_unambiguous(&self) -> Result<()> {
        for module in self.modules.values() {
            let mut visible_exports = HashMap::<&str, Vec<&str>>::new();
            for import in &module.imports {
                let imported = self.modules.get(import).expect("imports were validated");
                for export in &imported.exports {
                    visible_exports
                        .entry(export.as_str())
                        .or_default()
                        .push(import.as_str());
                }
            }

            for (provider, imports) in visible_exports {
                if imports.len() > 1 {
                    return Err(NidusError::AmbiguousProvider {
                        module: module.name.clone(),
                        provider: provider.to_owned(),
                        imports: imports.into_iter().map(str::to_owned).collect(),
                    });
                }
            }
        }
        Ok(())
    }

    fn visit(
        &self,
        name: &str,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
        stack: &mut Vec<String>,
    ) -> Result<()> {
        if visited.contains(name) {
            return Ok(());
        }

        if visiting.contains(name) {
            let cycle_start = stack.iter().position(|item| item == name).unwrap_or(0);
            let mut cycle = stack[cycle_start..].to_vec();
            cycle.push(name.to_owned());
            return Err(NidusError::CircularModuleImport { cycle });
        }

        visiting.insert(name.to_owned());
        stack.push(name.to_owned());
        if let Some(module) = self.modules.get(name) {
            for import in &module.imports {
                self.visit(import, visiting, visited, stack)?;
            }
        }
        stack.pop();
        visiting.remove(name);
        visited.insert(name.to_owned());
        Ok(())
    }
}
