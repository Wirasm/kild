//! ACP Client trait implementation.
//!
//! `KildAcpClient` implements the `agent_client_protocol::Client` trait,
//! forwarding events to the event channel for external consumption.

use std::pin::Pin;

use agent_client_protocol as acp;
use tokio::sync::mpsc;
use tracing::debug;

use crate::types::AcpEvent;

/// Implements the ACP Client trait, forwarding notifications and permission
/// requests to the event channel.
///
/// This runs on a single-threaded tokio runtime (inside a `LocalSet`), so it
/// does not need to be `Send`. The `!Send` futures produced by the trait are
/// fine in this context.
pub(crate) struct KildAcpClient {
    event_tx: mpsc::UnboundedSender<AcpEvent>,
}

impl KildAcpClient {
    pub(crate) fn new(event_tx: mpsc::UnboundedSender<AcpEvent>) -> Self {
        Self { event_tx }
    }
}

impl acp::Client for KildAcpClient {
    fn request_permission<'life0, 'async_trait>(
        &'life0 self,
        args: acp::RequestPermissionRequest,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<acp::RequestPermissionResponse, acp::Error>>
                + 'async_trait,
        >,
    >
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let payload = serde_json::to_string(&args).unwrap_or_default();
        let _ = self.event_tx.send(AcpEvent::PermissionRequest { payload });

        debug!(event = "acp.client.permission_requested");

        // Default: cancel permission. The UI layer (Phase 2) will handle
        // interactive permission flows by responding through the connection.
        Box::pin(async {
            Ok(acp::RequestPermissionResponse::new(
                acp::RequestPermissionOutcome::Cancelled,
            ))
        })
    }

    fn session_notification<'life0, 'async_trait>(
        &'life0 self,
        args: acp::SessionNotification,
    ) -> Pin<Box<dyn std::future::Future<Output = Result<(), acp::Error>> + 'async_trait>>
    where
        'life0: 'async_trait,
        Self: 'async_trait,
    {
        let payload = serde_json::to_string(&args).unwrap_or_default();
        let _ = self
            .event_tx
            .send(AcpEvent::SessionNotification { payload });
        Box::pin(async { Ok(()) })
    }
}
