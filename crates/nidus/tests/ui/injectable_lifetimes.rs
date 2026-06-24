use std::sync::Arc;

use nidus::prelude::*;

#[injectable(transient)]
#[derive(Debug)]
struct RequestId;

#[injectable(request)]
#[derive(Debug)]
struct RequestContext;

fn main() {
    let mut container = Container::new();
    RequestId::register_provider(&mut container).unwrap();
    RequestContext::register_provider(&mut container).unwrap();

    let first = container.resolve::<RequestId>().unwrap();
    let second = container.resolve::<RequestId>().unwrap();
    assert!(!Arc::ptr_eq(&first, &second));

    assert!(matches!(
        container.resolve::<RequestContext>().unwrap_err(),
        NidusError::RequestScopeRequired { .. }
    ));

    let scope = container.request_scope();
    let context = scope.resolve::<RequestContext>().unwrap();
    let context_again = scope.resolve::<RequestContext>().unwrap();
    assert!(Arc::ptr_eq(&context, &context_again));
}
