use super::types::Command;

/// Trait for dispatching business commands.
///
/// Decouples command definitions from their execution. Interfaces (CLI, UI)
/// implement this trait to execute commands with their specific needs
/// (e.g., UI adds event emission and async handling, CLI runs synchronously).
///
/// # Semantics
///
/// - **Ordering**: Commands execute in the order received. No implicit batching.
/// - **Idempotency**: Commands are not idempotent (e.g., `CreateKild` fails if
///   the branch already exists). Callers must avoid duplicate dispatches.
/// - **Error handling**: Implementations define their own error type. Errors
///   should distinguish user errors (invalid input) from system errors (IO failure).
pub trait Store {
    type Error;
    fn dispatch(&mut self, cmd: Command) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_trait_is_implementable() {
        struct TestStore;
        impl Store for TestStore {
            type Error = String;
            fn dispatch(&mut self, _cmd: Command) -> Result<(), String> {
                Ok(())
            }
        }
        let mut store = TestStore;
        assert!(store.dispatch(Command::RefreshSessions).is_ok());
    }

    #[test]
    fn test_store_impl_can_return_error() {
        struct FailingStore;
        impl Store for FailingStore {
            type Error = String;
            fn dispatch(&mut self, _cmd: Command) -> Result<(), String> {
                Err("not implemented".to_string())
            }
        }
        let mut store = FailingStore;
        assert!(store.dispatch(Command::RefreshSessions).is_err());
    }
}
