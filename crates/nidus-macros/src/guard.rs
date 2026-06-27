use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItemFn, Path, parse2};

use crate::utils::require_method_receiver;

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if let Err(error) = parse2::<Path>(attr) {
        return crate::diagnostics::compile_error_with_item_at(
            error.span(),
            "#[guard] requires a guard type like #[guard(AuthGuard)]",
            item,
        );
    }

    match parse2::<ImplItemFn>(item.clone()) {
        Ok(function) => match require_method_receiver(&function, "guard") {
            Ok(()) => quote!(#function),
            Err(error) => crate::diagnostics::compile_error_with_item(error.to_string(), item),
        },
        Err(error) => crate::diagnostics::compile_error_with_item(
            format!("#[guard] can only be used on route methods: {error}"),
            item,
        ),
    }
}
