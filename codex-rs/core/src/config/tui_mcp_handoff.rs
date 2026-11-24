//! Prepares a transport-only update for an existing viewer-owned registration.
//! Authentication of the replacement viewer belongs to the app-server boundary.

use super::Config;
use codex_config::McpServerAuth;
use codex_config::McpServerTransportConfig;
use std::collections::HashMap;
use std::io;

fn invalid() -> io::Error {
    // Do not include URLs, bearer credentials or serialized configuration.
    io::Error::other("TUI MCP handoff does not match the existing local registration")
}

fn valid_endpoint(value: &str) -> bool {
    let Ok(url) = url::Url::parse(value) else {
        return false;
    };
    url.scheme() == "http"
        && url.host_str() == Some("127.0.0.1")
        && url.port().is_some_and(|port| port != 0)
        && url.path() == "/mcp"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.as_str() == value
}

fn valid_authorization(value: &str) -> bool {
    value
        .strip_prefix("Bearer ")
        .and_then(|token| uuid::Uuid::parse_str(token).ok())
        .is_some_and(|token| token.get_version_num() == 4)
}

impl Config {
    /// Prepares an immutable candidate, preserving every non-transport setting.
    /// Publication must compare the captured Config owner, and the caller must
    /// authenticate the new endpoint and verify its project before calling this.
    pub fn with_tui_mcp_transport(
        &self,
        expected_url: &str,
        url: &str,
        authorization: &str,
    ) -> io::Result<Self> {
        if !valid_endpoint(expected_url)
            || !valid_endpoint(url)
            || !valid_authorization(authorization)
        {
            return Err(invalid());
        }
        let mut servers = self.mcp_servers.get().clone();
        let server = servers.get_mut("codex_tui").ok_or_else(invalid)?;
        if !server.enabled
            || server.auth != McpServerAuth::default()
            || !server.is_local_environment()
            || server.ema_registration().is_some()
        {
            return Err(invalid());
        }
        let McpServerTransportConfig::StreamableHttp {
            url: old_url,
            bearer_token_env_var,
            http_headers,
            env_http_headers,
            http_headers_helper,
        } = &server.transport
        else {
            return Err(invalid());
        };
        if old_url != expected_url
            || bearer_token_env_var.is_some()
            || env_http_headers.is_some()
            || http_headers_helper.is_some()
            || !http_headers.as_ref().is_some_and(|headers| {
                headers.len() == 1
                    && headers
                        .get("Authorization")
                        .is_some_and(|value| valid_authorization(value))
            })
        {
            return Err(invalid());
        }
        server.transport = McpServerTransportConfig::StreamableHttp {
            url: url.to_owned(),
            bearer_token_env_var: None,
            http_headers: Some(HashMap::from([(
                "Authorization".to_owned(),
                authorization.to_owned(),
            )])),
            env_http_headers: None,
            http_headers_helper: None,
        };
        let mut config = self.clone();
        config.config_layer_stack = self
            .config_layer_stack
            .clone()
            .with_session_tui_mcp_transport(expected_url, url, authorization)?;
        config.mcp_servers.set(servers).map_err(|_| invalid())?;
        if !config
            .mcp_servers
            .get()
            .get("codex_tui")
            .is_some_and(|server| server.enabled)
        {
            return Err(invalid());
        }
        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "http://127.0.0.1:54321/mcp";
    const NEW: &str = "http://127.0.0.1:54322/mcp";
    const AUTH: &str = "Bearer 550e8400-e29b-41d4-a716-446655440000";

    async fn fixture() -> (tempfile::TempDir, Config) {
        let home = tempfile::tempdir().unwrap();
        let overrides: toml::Table = toml::from_str(&format!(
            r#"
model = "original-model"
[mcp_servers.codex_tui]
url = "{OLD}"
default_tools_approval_mode = "prompt"
disabled_tools = ["create_worktree"]
[mcp_servers.codex_tui.http_headers]
Authorization = "{AUTH}"
[mcp_servers.codex_tui.tools.send_message_to_thread]
approval_mode = "prompt"
[mcp_servers.other]
url = "https://example.invalid/mcp"
"#
        ))
        .unwrap();
        let config = super::super::ConfigBuilder::without_managed_config_for_tests()
            .codex_home(home.path().to_path_buf())
            .fallback_cwd(Some(home.path().to_path_buf()))
            .cli_overrides(overrides.into_iter().collect())
            .build()
            .await
            .unwrap();
        (home, config)
    }

    #[tokio::test]
    async fn replacement_preserves_approvals_other_servers_and_session_settings() {
        let (_home, config) = fixture().await;
        let before = config.config_layer_stack.effective_config();
        let next = config.with_tui_mcp_transport(OLD, NEW, AUTH).unwrap();
        let mut expected = config.mcp_servers.get()["codex_tui"].clone();
        expected.transport = next.mcp_servers.get()["codex_tui"].transport.clone();
        assert_eq!(expected, next.mcp_servers.get()["codex_tui"]);
        assert_eq!(
            config.mcp_servers.get()["other"],
            next.mcp_servers.get()["other"]
        );
        assert_eq!(config.model, next.model);
        assert_eq!(config.cwd, next.cwd);
        assert_eq!(config.codex_home, next.codex_home);
        let mut after = next.config_layer_stack.effective_config();
        after["mcp_servers"]["codex_tui"]["url"] = OLD.into();
        assert_eq!(before, after);
        assert_eq!(
            config.config_layer_stack.effective_config()["mcp_servers"]["codex_tui"]["url"]
                .as_str(),
            Some(OLD)
        );
    }

    #[tokio::test]
    async fn changed_owner_and_non_native_registrations_are_refused() {
        let (_home, mut config) = fixture().await;
        assert!(config.with_tui_mcp_transport(NEW, OLD, AUTH).is_err());
        let mut servers = config.mcp_servers.get().clone();
        servers.get_mut("codex_tui").unwrap().auth = McpServerAuth::ChatGpt;
        config.mcp_servers.set(servers).unwrap();
        assert!(config.with_tui_mcp_transport(OLD, NEW, AUTH).is_err());
    }

    #[tokio::test]
    async fn a_managed_url_restriction_cannot_be_bypassed_by_rebinding() {
        let (_home, mut config) = fixture().await;
        config.mcp_servers = super::super::Constrained::normalized(
            config.mcp_servers.get().clone(),
            |mut servers| {
                let server = servers.get_mut("codex_tui").unwrap();
                if matches!(&server.transport, McpServerTransportConfig::StreamableHttp { url, .. } if url != OLD) {
                    server.enabled = false;
                }
                servers
            },
        ).unwrap();
        assert!(config.with_tui_mcp_transport(OLD, NEW, AUTH).is_err());
        assert!(config.mcp_servers.get()["codex_tui"].enabled);
    }

    #[test]
    fn endpoint_admission_is_canonical_loopback_only() {
        assert!(valid_endpoint("http://127.0.0.1:54321/mcp"));
        for endpoint in [
            "http://localhost:54321/mcp",
            "http://127.1:54321/mcp",
            "http://127.0.0.1:0/mcp",
            "https://127.0.0.1:54321/mcp",
            "http://127.0.0.1:54321/mcp?token=x",
            "http://user@127.0.0.1:54321/mcp",
            "http://127.0.0.1:54321/elsewhere",
            "http://192.0.2.1:54321/mcp",
        ] {
            assert!(!valid_endpoint(endpoint));
        }
    }

    #[test]
    fn authorization_requires_the_native_random_capability_format() {
        assert!(valid_authorization(
            "Bearer 550e8400-e29b-41d4-a716-446655440000"
        ));
        assert!(!valid_authorization("Bearer [redacted]"));
        assert!(!valid_authorization(
            "Bearer 00000000-0000-0000-0000-000000000000"
        ));
        assert!(!valid_authorization(
            "Bearer 550e8400-e29b-41d4-a716-446655440000\r\n"
        ));
    }
}
