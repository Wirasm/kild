//! Shared view helpers.

use chrono::{DateTime, Utc};

/// Format RFC3339 timestamp as relative time (e.g., "5m ago", "2h ago").
pub fn format_relative_time(timestamp: &str) -> String {
    let Ok(created) = DateTime::parse_from_rfc3339(timestamp) else {
        tracing::debug!(
            event = "ui.time.timestamp_parse_failed",
            timestamp = timestamp,
            "Failed to parse timestamp - displaying raw value"
        );
        return timestamp.to_string();
    };

    let now = Utc::now();
    let duration = now.signed_duration_since(created.with_timezone(&Utc));

    let minutes = duration.num_minutes();
    let hours = duration.num_hours();
    let days = duration.num_days();

    if days > 0 {
        format!("{}d ago", days)
    } else if hours > 0 {
        format!("{}h ago", hours)
    } else if minutes > 0 {
        format!("{}m ago", minutes)
    } else {
        "just now".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_relative_time_invalid_timestamp() {
        assert_eq!(format_relative_time("not-a-timestamp"), "not-a-timestamp");
    }

    #[test]
    fn test_format_relative_time_just_now() {
        let now = Utc::now().to_rfc3339();
        assert_eq!(format_relative_time(&now), "just now");
    }

    #[test]
    fn test_format_relative_time_minutes_ago() {
        use chrono::Duration;
        let five_min_ago = (Utc::now() - Duration::minutes(5)).to_rfc3339();
        assert_eq!(format_relative_time(&five_min_ago), "5m ago");
    }

    #[test]
    fn test_format_relative_time_hours_ago() {
        use chrono::Duration;
        let two_hours_ago = (Utc::now() - Duration::hours(2)).to_rfc3339();
        assert_eq!(format_relative_time(&two_hours_ago), "2h ago");
    }

    #[test]
    fn test_format_relative_time_days_ago() {
        use chrono::Duration;
        let three_days_ago = (Utc::now() - Duration::days(3)).to_rfc3339();
        assert_eq!(format_relative_time(&three_days_ago), "3d ago");
    }
}
