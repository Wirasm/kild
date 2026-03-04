#[cfg(test)]
mod tests {
    use super::super::*;

    fn make_payload(event: &str, branch: Option<&str>, summary: Option<&str>) -> HookPayload {
        HookPayload {
            hook_event_name: event.to_string(),
            session_id: branch.map(|b| b.to_string()),
            transcript_summary: summary.map(|s| s.to_string()),
            stop_hook_active: None,
        }
    }

    #[test]
    fn stop_event_sets_idle_status() {
        let payload = make_payload("Stop", Some("worker-a"), None);
        let result = process_hook(&payload, false);
        assert_eq!(result.agent_status.as_deref(), Some("idle"));
        assert_eq!(result.event_tag.as_deref(), Some("agent.stop"));
        assert!(result.forward_to_brain);
        assert!(result.should_gate);
    }

    #[test]
    fn stop_event_does_not_forward_for_brain() {
        let payload = make_payload("Stop", Some("honryu"), None);
        let result = process_hook(&payload, false);
        assert_eq!(result.agent_status.as_deref(), Some("idle"));
        assert!(
            !result.forward_to_brain,
            "Brain should not forward to itself"
        );
    }

    #[test]
    fn subagent_stop_suppressed_without_verbose() {
        let payload = make_payload("SubagentStop", Some("worker-a"), None);
        let result = process_hook(&payload, false);
        assert_eq!(result.agent_status.as_deref(), Some("idle"));
        assert!(
            !result.forward_to_brain,
            "SubagentStop should not forward without verbose"
        );
    }

    #[test]
    fn subagent_stop_forwards_with_verbose() {
        let payload = make_payload("SubagentStop", Some("worker-a"), None);
        let result = process_hook(&payload, true);
        assert_eq!(result.agent_status.as_deref(), Some("idle"));
        assert!(
            result.forward_to_brain,
            "SubagentStop should forward with verbose"
        );
        assert_eq!(result.event_tag.as_deref(), Some("subagent.stop"));
    }

    #[test]
    fn unknown_event_returns_empty_response() {
        let payload = make_payload("SomeNewEvent", Some("worker-a"), None);
        let result = process_hook(&payload, false);
        assert!(result.agent_status.is_none());
        assert!(!result.forward_to_brain);
    }

    #[test]
    fn stop_event_without_branch_does_not_forward() {
        let payload = make_payload("Stop", None, None);
        let result = process_hook(&payload, false);
        assert_eq!(result.agent_status.as_deref(), Some("idle"));
        assert!(!result.forward_to_brain, "No branch means no forwarding");
    }

    #[test]
    fn response_is_non_blocking_by_default() {
        let payload = make_payload("Stop", Some("worker-a"), None);
        let result = process_hook(&payload, false);
        assert!(
            result.response.decision.is_none(),
            "Stop events should not block via HTTP"
        );
    }
}
