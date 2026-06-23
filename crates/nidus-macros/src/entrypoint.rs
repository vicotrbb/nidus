use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, parse2};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    if !attr.is_empty() {
        return crate::diagnostics::compile_error_with_item(
            "#[nidus::main] does not accept arguments",
            item,
        );
    }

    let Ok(mut function) = parse2::<ItemFn>(item.clone()) else {
        return crate::diagnostics::compile_error_with_item(
            "#[nidus::main] can only be used on async functions",
            item,
        );
    };

    if function.sig.asyncness.take().is_none() {
        return crate::diagnostics::compile_error_with_item(
            "#[nidus::main] requires an async function",
            quote!(#function),
        );
    }

    let attrs = &function.attrs;
    let vis = &function.vis;
    let sig = &function.sig;
    let block = &function.block;

    quote! {
        #(#attrs)*
        #vis #sig {
            ::nidus::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("failed to build Nidus Tokio runtime")
                .block_on(async move #block)
        }
    }
}
