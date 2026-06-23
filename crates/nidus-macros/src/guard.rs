use proc_macro2::TokenStream;
use quote::quote;
use syn::{ImplItemFn, Path, parse2};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if parse2::<Path>(attr).is_err() {
        return crate::diagnostics::compile_error_with_item(
            "#[guard] requires a guard type like #[guard(AuthGuard)]",
            item,
        );
    }

    if parse2::<ImplItemFn>(item.clone()).is_err() {
        return crate::diagnostics::compile_error_with_item(
            "#[guard] can only be used on route methods",
            item,
        );
    }

    quote!(#item)
}
