# Same-thread TUI task-tools handoff prototype

Tracks [WebMux issue 707](https://github.com/jordanhubbard/webmux/issues/707).
This is a downstream native prototype, not a deployed fix or a four-session
survival qualification.

An already-loaded thread ignores resume configuration overrides. Its original
`codex_tui` registration can therefore continue to reference a retired viewer.
The experimental `thread/tuiMcp/rebind` operation updates that existing thread's
transport rather than creating a conversation, replaying a turn, or rewriting
the user's MCP configuration.

The replacement must be reached through an embedded/stdio connection or a
same-user Unix control socket. TCP WebSocket and remote-control origins are
refused. Windows control sockets retain their previous trust classification;
Windows handoff support is not qualified by this prototype. The Unix listener
checks the kernel-reported peer UID before assigning local-socket authority.
The separate native user-verification restrictions continue to reject socket
callers, including the new local-socket origin.

Only an enabled, local, session-owned `codex_tui` registration with the existing
native random bearer format may change. Its new URL must be canonical numeric
loopback HTTP on a nonzero explicit port at `/mcp`. The daemon makes an
authenticated, bounded `/tui-handoff` callback and verifies its canonical local
project directory against both the request and live thread. The callback does
not follow redirects, use a proxy, or log transport details. No credential is
returned by the operation or included in errors.

The candidate clones the existing configuration and changes only URL and
Authorization. Tool approvals, allow/deny lists, other MCP servers, model,
permissions, project, worktree configuration and managed restrictions remain
owned by the thread. The session-layer transport changes with the runtime map,
so a subsequent normal MCP reload uses the new endpoint. Publication compares
the captured configuration owner and current cloud-policy binding; a conflict
is reported without retrying. Running work is not interrupted. Already-sent
tool calls retain their original client and are never replayed.

On resume, a TUI that owns a task-tools server requests the handoff. Failure
leaves coding available, logs that management transport is unconfirmed, and
suppresses that viewer's positive task-tools capability claim. An older daemon
does not gain support through fallback, configuration writes or request replay.

Added native tests cover endpoint admission, managed URL restrictions,
preservation of non-transport configuration, stale publication, reload
persistence, same-user socket credentials, network-origin refusal, and a real
MCP `read_thread` handled by a replacement viewer after retiring the first
listener. The latter uses a handler counter to reject cached-old-connection
false positives. It is an embedded native regression, not evidence of four live
coding sessions surviving a daemon or WebMux disruption.

Full native compilation/tests, actual same-daemon viewer migration, active-turn
continuity, worktree-tool parity, interactive selection switches, and the full
four-session restart/crash/reconnect qualification remain gates before release
or activation. No currently running user daemon or viewer is replaced by this
source change.
