use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

/// Initialize logging with optional quiet mode.
///
/// When `quiet` is true, only error-level events are emitted.
/// When `quiet` is false, info-level and above events are emitted (default).
pub fn init_logging(quiet: bool) {
    let directive = if quiet {
        "kild_peek=error"
    } else {
        "kild_peek=info"
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .json()
                .with_writer(std::io::stderr)
                .with_current_span(false)
                .with_span_list(false),
        )
        .with(
            EnvFilter::from_default_env()
                .add_directive(directive.parse().expect("Invalid log directive")),
        )
        .init();
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_init_logging() {
        // Test that init_logging doesn't panic
        // Note: Can only call once per test process, so we can't actually test it here.
        // The function is tested via the CLI integration tests.
    }
}
