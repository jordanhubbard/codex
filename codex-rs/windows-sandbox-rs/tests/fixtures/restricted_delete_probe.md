# Restricted-token deletion diagnostic (issue jordanhubbard/webmux#742)

Run `powershell.exe -NoProfile -NonInteractive -File restricted_delete_probe.ps1`
on a disposable native Windows worker. This standalone probe uses only its own
random temporary directories and in-memory token/security descriptors. It reverts
impersonation before cleaning up. It changes no repository ACL or service.

The first matrix compares full restriction (flags 5) with write restriction
(flags 13) on a descriptor granting only the current user full access. The latter
omits the restricting-SID check for FILE_DELETE_CHILD (0x40), while still checking
DELETE (0x10000). A separate actual DeleteFile comparison changes only the parent
FILE_DELETE_CHILD permission and records the operation results.

The low-integrity experiment rejects an incomplete mitigation: low integrity can
protect an outside medium-integrity parent, but a low-integrity writable parent
still permits removal of a medium-integrity protected `.git` child despite the
capability deny. Thus lowering integrity alone does not meet the original test.

This is diagnostic evidence, not a product fix or a passing security gate. The
original `legacy_workspace_write_delete_is_limited_to_writable_roots` test remains
unchanged. A correction must retain required read access, permit writes/deletes
in the selected workspace and workload TEMP/TMP, and deny both outside deletion
and deletion of protected children. Do not obtain green by changing that contract.
