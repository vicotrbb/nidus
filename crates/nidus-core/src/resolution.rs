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

#[cfg(test)]
mod tests {
    use std::any::TypeId;

    use super::{enter, is_active};

    #[test]
    fn nested_resolution_guards_restore_the_stack() {
        let outer_type = TypeId::of::<u32>();
        let inner_type = TypeId::of::<u64>();
        let outer = enter(outer_type, "u32").unwrap();
        let inner = enter(inner_type, "u64").unwrap();

        assert!(is_active(outer_type));
        assert!(is_active(inner_type));
        drop(inner);
        assert!(is_active(outer_type));
        assert!(!is_active(inner_type));
        drop(outer);
        assert!(!is_active(outer_type));
    }

    #[test]
    fn out_of_order_guard_drop_keeps_other_entries_active() {
        let first_type = TypeId::of::<u32>();
        let second_type = TypeId::of::<u64>();
        let first = enter(first_type, "u32").unwrap();
        let second = enter(second_type, "u64").unwrap();

        drop(first);
        assert!(!is_active(first_type));
        assert!(is_active(second_type));
        drop(second);
        assert!(!is_active(second_type));
    }
}
