//! A fresh proc mount must not reopen an explicit filesystem denial.

use super::LONG_TIMEOUT_MS;
use super::codex_linux_sandbox_exe;
use super::should_skip_bwrap_tests;
use codex_protocol::models::PermissionProfile;
use codex_protocol::permissions::FileSystemAccessMode;
use codex_protocol::permissions::FileSystemSandboxEntry;
use codex_protocol::permissions::FileSystemSandboxPolicy;
use codex_protocol::permissions::FileSystemSandboxPolicyContext;
use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::time::Duration;
use tokio::process::Command;

#[test_case::test_case(false, false; "read_only_fresh_proc")]
#[test_case::test_case(false, true; "read_only_legacy_proc")]
#[test_case::test_case(true, false; "approved_root_write_fresh_proc")]
#[test_case::test_case(true, true; "approved_root_write_legacy_proc")]
#[tokio::test]
async fn explicit_proc_denial_survives_filesystem_binds(root_write: bool, no_proc: bool) {
    if should_skip_bwrap_tests().await {
        return;
    }
    let cwd = tempfile::tempdir().expect("isolated command directory");
    let mut policy = FileSystemSandboxPolicy::read_only();
    policy.entries.push(FileSystemSandboxEntry::new(
        AbsolutePathBuf::from_absolute_path("/proc").unwrap().into(),
        FileSystemAccessMode::Deny,
    ));
    if root_write {
        // Exercise the real approved-command root bind, without requesting
        // automatic read-only metadata mounts at nonexistent host-root paths.
        let cwd_uri = AbsolutePathBuf::from_absolute_path(cwd.path())
            .unwrap()
            .into();
        policy = policy.for_approved_command(&FileSystemSandboxPolicyContext {
            cwd: &cwd_uri,
            workspace_roots: std::slice::from_ref(&cwd_uri),
            user_home_dir: None,
            temporary_directories: Some(&[]),
        });
    }
    let profile =
        PermissionProfile::from_runtime_permissions(&policy, NetworkSandboxPolicy::Enabled);
    let profile = serde_json::to_string(&profile).unwrap();
    let mut command = Command::new(codex_linux_sandbox_exe());
    command
        .arg("--sandbox-policy-cwd")
        .arg(cwd.path())
        .arg("--permission-profile")
        .arg(profile)
        .current_dir(cwd.path())
        .env_clear()
        .env("PATH", "/usr/bin:/bin");
    if no_proc {
        command.arg("--no-proc");
    }
    command.args([
        "--",
        "/bin/sh",
        "-c",
        "if test -r /proc/self/cmdline || test -r /proc/self/fd/0; then printf 'proc reopened\\n'; exit 71; fi; printf 'command ran; proc denied\\n'",
    ]);
    let output = tokio::time::timeout(Duration::from_millis(LONG_TIMEOUT_MS), command.output())
        .await
        .expect("sandbox command deadline")
        .expect("sandbox command starts");
    assert!(
        output.status.success(),
        "stdout={}; stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert_eq!(output.stdout, b"command ran; proc denied\n");
    assert!(output.stderr.is_empty(), "{:?}", output.stderr);
}
