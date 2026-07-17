use std::collections::{BTreeMap, BTreeSet, btree_map::Entry};

use crate::{Module, NidusError, Result};

use super::ModuleDefinition;

enum VisibleProvider<'a> {
    Unique(&'a str),
    Ambiguous(Vec<&'a str>),
}

/// Validated graph of module definitions.
#[derive(Debug)]
pub struct ModuleGraph {
    modules: BTreeMap<String, ModuleDefinition>,
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
        collect_recursive(M::definition(), &mut definitions, &mut BTreeSet::new());
        for module in modules {
            collect_recursive(module, &mut definitions, &mut BTreeSet::new());
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
        graph.validate_local_imports_unique()?;
        graph.validate_imports_exist()?;
        graph.validate_acyclic()?;
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

    fn validate_local_imports_unique(&self) -> Result<()> {
        for module in self.modules.values() {
            let mut seen = BTreeSet::new();
            for import in &module.imports {
                if !seen.insert(import) {
                    return Err(NidusError::DuplicateModuleImport {
                        module: module.name.clone(),
                        import: import.clone(),
                    });
                }
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

    fn validate_acyclic(&self) -> Result<()> {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        let mut stack = Vec::new();

        for name in self.modules.keys() {
            self.visit(name, &mut visiting, &mut visited, &mut stack)?;
        }
        Ok(())
    }

    fn validate_local_providers_unique(&self) -> Result<()> {
        for module in self.modules.values() {
            let mut seen = BTreeSet::new();
            for provider in &module.providers {
                if !seen.insert(provider) {
                    return Err(NidusError::DuplicateModuleProvider {
                        module: module.name.clone(),
                        provider: provider.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_local_controllers_unique(&self) -> Result<()> {
        for module in self.modules.values() {
            let mut seen = BTreeSet::new();
            for controller in &module.controllers {
                if !seen.insert(controller) {
                    return Err(NidusError::DuplicateModuleController {
                        module: module.name.clone(),
                        controller: controller.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_providers_and_controllers_disjoint(&self) -> Result<()> {
        for module in self.modules.values() {
            let providers = module.providers.iter().collect::<BTreeSet<_>>();
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
            let mut seen = BTreeSet::new();
            for export in &module.exports {
                if !seen.insert(export) {
                    return Err(NidusError::DuplicateModuleExport {
                        module: module.name.clone(),
                        provider: export.clone(),
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_exports_are_local(&self) -> Result<()> {
        for module in self.modules.values() {
            let providers = module.providers.iter().collect::<BTreeSet<_>>();
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
            let local_providers = module.providers.iter().collect::<BTreeSet<_>>();
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
                self.visit(import, visiting, visited, stack)?;
            }
        }
        stack.pop();
        visiting.remove(name);
        visited.insert(name);
        Ok(())
    }
}

fn collect_recursive(
    module: ModuleDefinition,
    definitions: &mut Vec<ModuleDefinition>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(module.name().to_owned()) {
        return;
    }

    for import in module.import_factories() {
        collect_recursive(import(), definitions, seen);
    }

    definitions.push(module);
}
