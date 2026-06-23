use proc_macro2::TokenStream;
use syn::{LitStr, parse2};

pub(crate) fn require_empty_attr(attr: TokenStream, macro_name: &str) -> Result<(), TokenStream> {
    if attr.is_empty() {
        Ok(())
    } else {
        Err(crate::diagnostics::compile_error(format!(
            "#[{macro_name}] does not accept arguments yet"
        )))
    }
}

pub(crate) fn require_path_attr(
    attr: TokenStream,
    macro_name: &str,
) -> Result<LitStr, TokenStream> {
    parse2::<LitStr>(attr).map_err(|error| {
        crate::diagnostics::compile_error(format!(
            "#[{macro_name}] requires a string literal path like #[{macro_name}(\"/users\")]: {error}"
        ))
    })
}

pub(crate) fn validate_route_path(path: &LitStr) -> syn::Result<()> {
    for segment in path.value().split('/') {
        if segment == ":" {
            return Err(syn::Error::new(
                path.span(),
                "route path parameters must include a name after ':'",
            ));
        }
    }
    Ok(())
}
