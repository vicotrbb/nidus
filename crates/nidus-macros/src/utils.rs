use proc_macro2::TokenStream;
use syn::{ImplItemFn, LitStr, parse2, spanned::Spanned};

pub(crate) fn require_empty_attr(attr: TokenStream, macro_name: &str) -> Result<(), TokenStream> {
    if attr.is_empty() {
        Ok(())
    } else {
        Err(crate::diagnostics::compile_error_at(
            attr.span(),
            format!("#[{macro_name}] does not accept arguments yet"),
        ))
    }
}

pub(crate) fn require_path_attr(
    attr: TokenStream,
    macro_name: &str,
) -> Result<LitStr, TokenStream> {
    parse2::<LitStr>(attr).map_err(|error| {
        let message = format!(
            "#[{macro_name}] requires a string literal path like #[{macro_name}(\"/users\")]: {error}"
        );
        crate::diagnostics::compile_error_at(error.span(), message)
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

pub(crate) fn require_method_receiver(function: &ImplItemFn, macro_name: &str) -> syn::Result<()> {
    if function.sig.receiver().is_some() {
        Ok(())
    } else {
        Err(syn::Error::new_spanned(
            function.sig.ident.clone(),
            format!("#[{macro_name}] can only be used on route methods"),
        ))
    }
}
