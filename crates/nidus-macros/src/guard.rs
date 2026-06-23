use proc_macro2::TokenStream;
use quote::quote;

pub(crate) fn expand(_attr: TokenStream, item: TokenStream) -> TokenStream {
    quote!(#item)
}
