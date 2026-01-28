use tracing::{error, info};

pub fn log_app_startup() {
    info!(
        event = "core.app.startup_completed",
        version = env!("CARGO_PKG_VERSION")
    );
}

pub fn log_app_shutdown() {
    info!(event = "core.app.shutdown_started");
}

pub fn log_app_error(error: &dyn std::error::Error) {
    error!(
        event = "core.app.error_occurred",
        error = %error,
        error_type = std::any::type_name_of_val(error)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_events() {
        // Test that event functions don't panic
        log_app_startup();
        log_app_shutdown();

        let test_error = std::io::Error::new(std::io::ErrorKind::Other, "test");
        log_app_error(&test_error);
    }
}
