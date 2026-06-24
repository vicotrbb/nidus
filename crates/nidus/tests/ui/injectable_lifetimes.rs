use std::sync::Arc;

use nidus::prelude::*;

#[injectable(transient)]
#[derive(Debug)]
struct TransientId;

#[injectable(request)]
#[derive(Debug)]
struct RequestId;

#[injectable(request)]
#[derive(Debug)]
struct RequestContext {
    request_id: Inject<RequestId>,
}

fn main() {
    let mut container = Container::new();
    TransientId::register_provider(&mut container).unwrap();
    RequestId::register_provider(&mut container).unwrap();
    RequestContext::register_provider(&mut container).unwrap();

    let first = container.resolve::<TransientId>().unwrap();
    let second = container.resolve::<TransientId>().unwrap();
    assert!(!Arc::ptr_eq(&first, &second));

    assert!(matches!(
        container.resolve::<RequestContext>().unwrap_err(),
        NidusError::RequestScopeRequired { .. }
    ));

    let scope = container.request_scope();
    let context = scope.resolve::<RequestContext>().unwrap();
    let context_again = scope.resolve::<RequestContext>().unwrap();
    let request_id = scope.resolve::<RequestId>().unwrap();

    assert!(Arc::ptr_eq(&context, &context_again));
    assert!(Arc::ptr_eq(
        &context.request_id.clone().into_inner(),
        &request_id
    ));
}
