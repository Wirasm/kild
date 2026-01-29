use std::error::Error;

/// Base trait for all application errors
pub trait ShardsError: Error + Send + Sync + 'static {
    /// Error code for programmatic handling
    fn error_code(&self) -> &'static str;

    /// Whether this error should be logged as an error or warning
    fn is_user_error(&self) -> bool {
        false
    }
}

/// Common result type for the application
pub type ShardsResult<T> = Result<T, Box<dyn ShardsError>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shards_result() {
        let _result: ShardsResult<i32> = Ok(42);
    }
}
