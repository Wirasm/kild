use std::error::Error;

/// Base trait for all peek application errors
pub trait PeekError: Error + Send + Sync + 'static {
    /// Error code for programmatic handling
    fn error_code(&self) -> &'static str;

    /// Whether this error should be logged as an error or warning
    fn is_user_error(&self) -> bool {
        false
    }
}

/// Common result type for the application
pub type PeekResult<T> = Result<T, Box<dyn PeekError>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_peek_result() {
        let _result: PeekResult<i32> = Ok(42);
    }
}
