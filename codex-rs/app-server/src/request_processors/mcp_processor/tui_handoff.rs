//! Authenticated same-host replacement of a viewer-owned task-tools endpoint.

use super::McpRequestProcessor;
use crate::error_code::internal_error;
use crate::error_code::invalid_request;
use crate::transport::ConnectionOrigin;
use codex_app_server_protocol::ClientResponsePayload;
use codex_app_server_protocol::JSONRPCErrorError;
use codex_app_server_protocol::ThreadTuiMcpRebindParams;
use codex_app_server_protocol::ThreadTuiMcpRebindResponse;
use codex_config::McpServerTransportConfig;
use codex_core::ConfigRefreshOutcome;
use codex_http_client::HttpClientBuilder;
use codex_http_client::bounded_response_bytes;
use std::time::Duration;

fn rejected() -> JSONRPCErrorError {
    // Never attach raw transport errors, configuration, paths or credentials.
    invalid_request("TUI MCP handoff could not verify the same local project and endpoint")
}

impl McpRequestProcessor {
    pub(crate) async fn tui_mcp_rebind(
        &self,
        origin: ConnectionOrigin,
        params: ThreadTuiMcpRebindParams,
    ) -> Result<Option<ClientResponsePayload>, JSONRPCErrorError> {
        if !matches!(
            origin,
            ConnectionOrigin::InProcess | ConnectionOrigin::Stdio | ConnectionOrigin::LocalSocket
        ) {
            return Err(rejected());
        }
        let (_, thread) = self.load_thread(&params.thread_id).await?;
        let captured = thread.config().await;
        let old_url = match captured
            .mcp_servers
            .get()
            .get("codex_tui")
            .map(|server| &server.transport)
        {
            Some(McpServerTransportConfig::StreamableHttp { url, .. }) => url.clone(),
            _ => return Err(rejected()),
        };
        let authorization = format!("Bearer {}", params.bearer_token.as_str());
        // Validate the precise registration shape and managed restrictions before
        // making a connection, including a canonical numeric loopback URL.
        captured
            .with_tui_mcp_transport(&old_url, &params.url, &authorization)
            .map_err(|_| rejected())?;
        let verify = async {
            let project = tokio::fs::canonicalize(&captured.cwd)
                .await
                .map_err(|_| rejected())?;
            let viewer_project = tokio::fs::canonicalize(&params.cwd)
                .await
                .map_err(|_| rejected())?;
            if project != viewer_project {
                return Err(rejected());
            }
            let proof_url = params.url.strip_suffix("/mcp").ok_or_else(rejected)?;
            let proof_url = format!("{proof_url}/tui-handoff");
            // This is a local capability callback, never outbound product traffic.
            // Sending its bearer through a proxy or a redirect would leak authority.
            let client = HttpClientBuilder::new()
                .without_request_logging()
                .without_redirects()
                .build_direct()
                .map_err(|_| rejected())?;
            let response = client
                .get(&proof_url)
                .header("Authorization", authorization.as_str())
                .send()
                .await
                .map_err(|_| rejected())?;
            if !response.status().is_success() {
                return Err(rejected());
            }
            let body = bounded_response_bytes(response, 8192)
                .await
                .map_err(|_| rejected())?;
            let proof: serde_json::Value = serde_json::from_slice(&body).map_err(|_| rejected())?;
            let proof_cwd = proof
                .get("cwd")
                .and_then(serde_json::Value::as_str)
                .ok_or_else(rejected)?;
            if proof.get("schema").and_then(serde_json::Value::as_u64) != Some(1)
                || std::path::Path::new(proof_cwd) != project
            {
                return Err(rejected());
            }
            Ok(())
        };
        tokio::time::timeout(Duration::from_secs(2), verify)
            .await
            .map_err(|_| rejected())??;
        match thread
            .rebind_tui_mcp_transport(captured, &old_url, &params.url, &authorization)
            .await
            .map_err(|_| rejected())?
        {
            ConfigRefreshOutcome::Published => Ok(Some(ThreadTuiMcpRebindResponse {}.into())),
            ConfigRefreshOutcome::Stale => Err(internal_error(
                "TUI MCP handoff conflicted with a newer configuration; no handoff was published",
            )),
            ConfigRefreshOutcome::Rejected => Err(rejected()),
        }
    }
}
