//! Dependency resolution cycle tracking.

use std::{any::TypeId, cell::RefCell};

use crate::{NidusError, Result};

thread_local! {
    static RESOLUTION_STACK: RefCell<Vec<TypeId>> = const { RefCell::new(Vec::new()) };
}

/// Guard that removes a provider from the current thread's resolution stack.
pub(crate) struct ResolutionGuard {
    type_id: TypeId,
}

/// Enters provider resolution for the current thread.
pub(crate) fn enter(type_id: TypeId, type_name: &'static str) -> Result<ResolutionGuard> {
    if is_active(type_id) {
        return Err(NidusError::CircularProviderResolution { type_name });
    }

    RESOLUTION_STACK.with(|stack| stack.borrow_mut().push(type_id));
    Ok(ResolutionGuard { type_id })
}

/// Returns whether the provider is already resolving on the current thread.
pub(crate) fn is_active(type_id: TypeId) -> bool {
    RESOLUTION_STACK.with(|stack| stack.borrow().contains(&type_id))
}

impl Drop for ResolutionGuard {
    fn drop(&mut self) {
        RESOLUTION_STACK.with(|stack| {
            let mut stack = stack.borrow_mut();
            if let Some(index) = stack.iter().rposition(|type_id| type_id == &self.type_id) {
                stack.remove(index);
            }
        });
    }
}
