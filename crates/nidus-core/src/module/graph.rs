use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use crate::{Container, Module, NidusError, Result};

use super::ModuleDefinition;

enum VisibleProvider<'a> {
    Unique(&'a str),
    Ambiguous(Vec<&'a str>),
}

enum NameLookup<'a> {
    Empty,
    One(&'a str),
    Many(BTreeSet<&'a str>),
}

impl<'a> NameLookup<'a> {
    fn new(names: &'a [String]) -> Self {
        match names {
            [] => Self::Empty,
            [name] => Self::One(name),
            names => Self::Many(names.iter().map(String::as_str).collect()),
        }
    }

    fn contains(&self, name: &str) -> bool {
        match self {
            Self::Empty => false,
            Self::One(only) => *only == name,
            Self::Many(names) => names.contains(name),
        }
    }
}

/// Validated graph of module definitions.
pub struct ModuleGraph {
    modules: BTreeMap<String, ModuleDefinition>,
    dependency_order: Vec<String>,
}

impl std::fmt::Debug for ModuleGraph {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModuleGraph")
            .field("modules", &self.modules)
            .finish()
    }
}

impl ModuleGraph {
    /// Builds and validates a module graph by recursively following typed imports.
    pub fn from_root<M: Module>() -> Result<Self> {
        Self::from_root_and_modules::<M, _>([])
    }

    /// Builds and validates a module graph from a root module plus explicit definitions.
    pub fn from_root_and_modules<M, I>(modules: I) -> Result<Self>
    where
        M: Module,
        I: IntoIterator<Item = ModuleDefinition>,
    {
        let mut definitions = Vec::new();
        let mut seen = BTreeSet::new();
        collect_recursive(M::definition(), &mut definitions, &mut seen);
        for module in modules {
            collect_explicit(module, &mut definitions, &mut seen);
        }
        Self::from_modules(definitions)
    }

    /// Builds and validates a module graph.
    pub fn from_modules(modules: impl IntoIterator<Item = ModuleDefinition>) -> Result<Self> {
        let span = tracing::info_span!("module.graph.validate");
        let _entered = span.enter();
        let mut registered = BTreeMap::new();
        for module in modules {
            let name = module.name.clone();
            match registered.entry(name) {
                Entry::Vacant(entry) => {
                    entry.insert(module);
                }
                Entry::Occupied(entry) => {
                    return Err(NidusError::DuplicateModule {
                        module: entry.key().clone(),
                    });
                }
            }
        }
        let mut graph = Self {
            modules: registered,
            dependency_order: Vec::new(),
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
        graph.validate_local_imports_unique()?;
        graph.validate_imports_exist()?;
        graph.dependency_order = graph.validate_acyclic()?;
        graph.validate_local_providers_unique()?;
        graph.validate_local_controllers_unique()?;
        graph.validate_providers_and_controllers_disjoint()?;
        graph.validate_exports_unique()?;
        graph.validate_exports_are_local()?;
        graph.validate_local_providers_do_not_conflict_with_imports()?;
        graph.validate_visible_providers_unambiguous()?;
        tracing::debug!(module_count = graph.modules.len(), "module graph validated");
        Ok(graph)
    }

    /// Returns a module definition by name.
    pub fn get(&self, name: &str) -> Option<&ModuleDefinition> {
        self.modules.get(name)
    }

    /// Returns validated module definitions in deterministic name order.
    pub fn modules(&self) -> impl Iterator<Item = &ModuleDefinition> {
        self.modules.values()
    }

    /// Registers every typed provider in deterministic dependency order.
    ///
    /// Imported modules are registered before their importers. Every
    /// synchronous registrar completes before callers run
    /// [`Self::initialize_providers`].
    pub fn register_providers(&self, container: &mut Container) -> Result<()> {
        for module in self.modules_in_dependency_order() {
            for registrar in module.provider_registrars() {
                registrar(container)?;
            }
        }
        Ok(())
    }

    /// Runs every async provider initializer in deterministic dependency order.
    ///
    /// Imported modules initialize before their importers, allowing an importer
    /// initializer to resolve resources created by an imported initializer.
    /// Initializers remain sequential because each receives exclusive access to
    /// the dependency container.
    pub async fn initialize_providers(&self, container: &mut Container) -> Result<()> {
        for module in self.modules_in_dependency_order() {
            for initializer in module.async_initializers() {
                initializer(container).await?;
            }
        }
        Ok(())
    }

    fn modules_in_dependency_order(&self) -> impl Iterator<Item = &ModuleDefinition> {
        self.dependency_order.iter().map(|name| {
            self.modules
                .get(name)
                .expect("dependency order only contains validated modules")
        })
    }

    fn validate_local_imports_unique(&self) -> Result<()> {
        for module in self.modules.values() {
            if let Some(import) = first_duplicate(&module.imports) {
                return Err(NidusError::DuplicateModuleImport {
                    module: module.name.clone(),
                    import: import.to_owned(),
                });
            }
        }
        Ok(())
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

    fn validate_acyclic(&self) -> Result<Vec<String>> {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();
        let mut order = Vec::with_capacity(self.modules.len());

        for name in self.modules.keys() {
            self.visit(name, &mut visiting, &mut visited, &mut stack, &mut order)?;
        }
        Ok(order)
    }

    fn validate_local_providers_unique(&self) -> Result<()> {
        for module in self.modules.values() {
            if let Some(provider) = first_duplicate(&module.providers) {
                return Err(NidusError::DuplicateModuleProvider {
                    module: module.name.clone(),
                    provider: provider.to_owned(),
                });
            }
        }
        Ok(())
    }

    fn validate_local_controllers_unique(&self) -> Result<()> {
        for module in self.modules.values() {
            if let Some(controller) = first_duplicate(&module.controllers) {
                return Err(NidusError::DuplicateModuleController {
                    module: module.name.clone(),
                    controller: controller.to_owned(),
                });
            }
        }
        Ok(())
    }

    fn validate_providers_and_controllers_disjoint(&self) -> Result<()> {
        for module in self.modules.values() {
            if module.controllers.is_empty() {
                continue;
            }
            let providers = NameLookup::new(&module.providers);
            for controller in &module.controllers {
                if providers.contains(controller) {
                    return Err(NidusError::ModuleProviderControllerConflict {
                        module: module.name.clone(),
                        type_name: controller.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_exports_unique(&self) -> Result<()> {
        for module in self.modules.values() {
            if let Some(export) = first_duplicate(&module.exports) {
                return Err(NidusError::DuplicateModuleExport {
                    module: module.name.clone(),
                    provider: export.to_owned(),
                });
            }
        }
        Ok(())
    }

    fn validate_exports_are_local(&self) -> Result<()> {
        for module in self.modules.values() {
            if module.exports.is_empty() {
                continue;
            }
            let providers = NameLookup::new(&module.providers);
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

    fn validate_local_providers_do_not_conflict_with_imports(&self) -> Result<()> {
        for module in self.modules.values() {
            if module.imports.is_empty() {
                continue;
            }
            let local_providers = NameLookup::new(&module.providers);
            for import in &module.imports {
                let imported = self.modules.get(import).expect("imports were validated");
                for export in &imported.exports {
                    if local_providers.contains(export) {
                        return Err(NidusError::ProviderVisibilityConflict {
                            module: module.name.clone(),
                            provider: export.clone(),
                            import: import.clone(),
                        });
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_visible_providers_unambiguous(&self) -> Result<()> {
        for module in self.modules.values() {
            let mut visible_exports = BTreeMap::<&str, VisibleProvider<'_>>::new();
            for import in &module.imports {
                let imported = self.modules.get(import).expect("imports were validated");
                for export in &imported.exports {
                    match visible_exports.entry(export.as_str()) {
                        Entry::Vacant(entry) => {
                            entry.insert(VisibleProvider::Unique(import));
                        }
                        Entry::Occupied(mut entry) => {
                            let visible_provider = entry.get_mut();
                            match visible_provider {
                                VisibleProvider::Unique(first_import) => {
                                    *visible_provider =
                                        VisibleProvider::Ambiguous(vec![*first_import, import]);
                                }
                                VisibleProvider::Ambiguous(imports) => imports.push(import),
                            }
                        }
                    }
                }
            }

            for (provider, visibility) in visible_exports {
                if let VisibleProvider::Ambiguous(imports) = visibility {
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

    fn visit<'a>(
        &'a self,
        name: &'a str,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
        stack: &mut Vec<&'a str>,
        order: &mut Vec<String>,
    ) -> Result<()> {
        if visited.contains(name) {
            return Ok(());
        }

        if visiting.contains(name) {
            let cycle_start = stack.iter().position(|item| *item == name).unwrap_or(0);
            let mut cycle = stack[cycle_start..]
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>();
            cycle.push(name.to_owned());
            return Err(NidusError::CircularModuleImport { cycle });
        }

        visiting.insert(name);
        stack.push(name);
        if let Some(module) = self.modules.get(name) {
            for import in &module.imports {
                self.visit(import, visiting, visited, stack, order)?;
            }
        }
        stack.pop();
        visiting.remove(name);
        visited.insert(name);
        order.push(name.to_owned());
        Ok(())
    }
}

fn first_duplicate(names: &[String]) -> Option<&str> {
    if names.len() < 2 {
        return None;
    }

    let mut seen = BTreeSet::new();
    names
        .iter()
        .find(|name| !seen.insert(name.as_str()))
        .map(String::as_str)
}

fn collect_recursive(
    module: ModuleDefinition,
    definitions: &mut Vec<ModuleDefinition>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(module.name().to_owned()) {
        return;
    }

    collect_new(module, definitions, seen);
}

fn collect_explicit(
    module: ModuleDefinition,
    definitions: &mut Vec<ModuleDefinition>,
    seen: &mut BTreeSet<String>,
) {
    if seen.insert(module.name().to_owned()) {
        collect_new(module, definitions, seen);
    } else {
        // Keep explicit duplicates visible to `from_modules`, while allowing
        // separate typed subgraphs to share recursively discovered dependencies.
        definitions.push(module);
    }
}

fn collect_new(
    module: ModuleDefinition,
    definitions: &mut Vec<ModuleDefinition>,
    seen: &mut BTreeSet<String>,
) {
    for import in module.import_factories() {
        collect_recursive(import(), definitions, seen);
    }

    definitions.push(module);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ModuleBuilder;

    #[test]
    fn dependency_order_is_stable_for_transitive_and_diamond_imports() {
        let core = ModuleBuilder::new("CoreModule").build();
        let left = ModuleBuilder::new("LeftModule")
            .import("CoreModule")
            .build();
        let right = ModuleBuilder::new("RightModule")
            .import("CoreModule")
            .build();
        let root = ModuleBuilder::new("RootModule")
            .import("RightModule")
            .import("LeftModule")
            .build();
        let independent = ModuleBuilder::new("ZIndependentModule").build();

        let graph = ModuleGraph::from_modules([root, right, independent, left, core]).unwrap();

        assert_eq!(
            graph.dependency_order,
            [
                "CoreModule",
                "LeftModule",
                "RightModule",
                "RootModule",
                "ZIndependentModule",
            ]
        );
    }
}
