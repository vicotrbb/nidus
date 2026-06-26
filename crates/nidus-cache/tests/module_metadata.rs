use nidus_cache::MokaCacheProvider;
use nidus_core::ModuleBuilder;

#[test]
fn moka_cache_module_metadata_is_typed_and_exported() {
    let module = ModuleBuilder::new("CacheModule")
        .provider_typed::<MokaCacheProvider>()
        .export_typed::<MokaCacheProvider>()
        .build();

    assert_eq!(module.providers(), ["MokaCacheProvider"]);
    assert_eq!(module.exports(), ["MokaCacheProvider"]);
}
