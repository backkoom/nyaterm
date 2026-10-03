# NyaTerm plugin system implementation

The requested plugin system builds on the preceding architecture review. The implementation must provide a usable extension host, local package installation, lifecycle management, permissions, isolated UI, native runtime, documentation and an end-to-end example.

Implemented as the local tool/panel extension host v1. See the [plugin guide](plugins.md)
and [validation report](plugin-system-validation.md) for the supported APIs, evidence
and remaining desktop release QA limits.

## Completion requirements

- A versioned, validated manifest declares panels, commands, menu locations, permissions and optional platform-specific native backends.
- `.nyap` packages can be inspected before installation, safely extracted with strict size/path/checksum validation, installed as immutable versions, enabled/disabled, updated, rolled back and uninstalled.
- Permissions are granted explicitly, enforced in Rust, and checked together with live session scope and application lock state. Native execution is presented as trusted code with OS user permissions.
- Plugin panels integrate with the existing activity bar and panel layouts; plugin commands are discoverable and available in declared context menus.
- Third-party HTML/JS runs in a sandboxed iframe with a restricted CSP and a bridge bound to its own iframe, plugin identity and backend-issued scope token. It cannot invoke arbitrary Tauri commands.
- The host API supports scoped session metadata, recent terminal output, approved command execution, scoped file reads, namespaced settings and declared network origins. Revocation, session closure and window teardown invalidate access.
- Optional sidecars use lazy persistent processes, a versioned initialization handshake, bounded JSON-RPC/binary frames, timeouts, crash cleanup and stop-on-disable/update behavior.
- Updates and removal do not race active requests; plugin state and permissions survive restarts. Plugin layouts survive temporary unavailability.
- A JavaScript SDK, packager, working example plugin and developer documentation cover the installation and authoring workflow.
- Meaningful backend and frontend tests, type checking, relevant linting and a build verify the implementation. A runtime smoke check verifies installation and UI/bridge integration where the environment permits.

New connection protocols and replacing built-in SSH/SFTP/RDP/VNC are subsequent provider API work described by the review, rather than requirements of this tool/panel extension host. A remote marketplace is also a separate service. Local packages and the extension runtime must be complete independently.
