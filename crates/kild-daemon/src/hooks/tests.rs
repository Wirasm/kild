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
        assert_eq!(result.agent_status, Some(kild_core::AgentStatus::Idle));
        let fwd = result.forward.as_ref().expect("should forward");
        assert_eq!(fwd.event_tag, "agent.stop");
        assert_eq!(fwd.branch, "worker-a");
        assert!(fwd.gated);
    }

    #[test]
    fn stop_event_does_not_forward_for_brain() {
        let payload = make_payload("Stop", Some("honryu"), None);
        let result = process_hook(&payload, false);
        assert_eq!(result.agent_status, Some(kild_core::AgentStatus::Idle));
        assert!(
            result.forward.is_none(),
            "Brain should not forward to itself"
        );
    }

    #[test]
    fn subagent_stop_suppressed_without_verbose() {
        let payload = make_payload("SubagentStop", Some("worker-a"), None);
        let result = process_hook(&payload, false);
        assert_eq!(result.agent_status, Some(kild_core::AgentStatus::Idle));
        assert!(
            result.forward.is_none(),
            "SubagentStop should not forward without verbose"
        );
    }

    #[test]
    fn subagent_stop_forwards_with_verbose() {
        let payload = make_payload("SubagentStop", Some("worker-a"), None);
        let result = process_hook(&payload, true);
        assert_eq!(result.agent_status, Some(kild_core::AgentStatus::Idle));
        let fwd = result
            .forward
            .as_ref()
            .expect("should forward with verbose");
        assert_eq!(fwd.event_tag, "subagent.stop");
        assert!(!fwd.gated);
    }

    #[test]
    fn unknown_event_returns_empty_response() {
        let payload = make_payload("SomeNewEvent", Some("worker-a"), None);
        let result = process_hook(&payload, false);
        assert!(result.agent_status.is_none());
        assert!(result.forward.is_none());
    }

    #[test]
    fn stop_event_without_branch_does_not_forward() {
        let payload = make_payload("Stop", None, None);
        let result = process_hook(&payload, false);
        assert_eq!(result.agent_status, Some(kild_core::AgentStatus::Idle));
        assert!(result.forward.is_none(), "No branch means no forwarding");
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
