# Private TUI selection observation

`--selection-observer-fd FD` is an optional Unix launcher integration. The
launcher supplies the write end of its own anonymous pipe as an inherited file
descriptor numbered at least 3. The TUI validates that descriptor, marks it
nonblocking and close-on-exec before app-server bootstrap, and owns its lifetime.
An invalid descriptor causes startup to fail; observation errors after startup
disable observation without interrupting coding.

Each newline-delimited record is at most 512 bytes:

```json
{"schema":"codex.tui.selection.v1","pid":1234,"sequence":1,"thread_id":null}
```

`sequence` strictly increases for successfully written records. `thread_id` is
either the settled displayed thread's UUID or null. A launcher must independently
bind the pipe and PID to the child it launched and verify the selected thread in
that execution account's native store. This is a one-way observation channel,
not an execution or management interface. It contains no prompts, paths, bearer
tokens, terminal content, or instructions to replay.

Selection is reported only when the active thread and transcript agree, replay
has settled, the view is live, the backend is connected, and execution is local.
Local execution means an embedded backend or an owner-only Unix socket owned by
the effective account. WebSocket backends, including loopback WebSockets, and
cloud backends report null. A transition or disconnected view also reports null.
Unchanged state is not repeated. Backpressure defers publication and cannot block
the UI. Lost or malformed observation must remove the launcher's confidence;
the previous UUID can remain available for an explicit recovery choice.

This does not transfer the viewer-owned `codex_tui` MCP server, repair a retired
tool endpoint, or attest uninterrupted daemon execution. Those require separate
handoff and process-lifetime evidence.
