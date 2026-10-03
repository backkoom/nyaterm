# Plugin v1 validation

Developer tooling and diagnostics were subsequently completed; see
[the development guide](plugin-development.md) and the updated verification below.

Validated on Windows on 2026-10-03. Scope follows
[the implementation plan](plugin-system-plan.md).

| Requirement                          | Implementation and evidence                                                                                                                                                                                                                                                                      |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| Versioned manifest and contributions | Independent runtime validates API/engine versions, paths, permissions, panels and command/menu declarations. Frontend contribution tests verify enabled-version discovery and namespaced IDs.                                                                                                    |
| Local package lifecycle              | Package/registry tests cover traversal, case collisions, complete checksums, modified content, inspection digest, immutable versions, persisted grants, permission revocation, update, rollback and publisher continuity.                                                                        |
| Permissions and scope                | Rust gateway allowlists capabilities and combines grants with plugin version, window ownership, connected session and lock checks. Approved command/file operations revalidate scope before execution. Permission review is bound to the active version to reject stale dialogs after an update. |
| Isolated UI                          | Bridge tests cover unrelated/spoofed frames, opaque origin, fixed scope token, arbitrary native command attempts, duplicate IDs, oversized messages and disposal. IPC tests cover top-frame initialization and Tauri binary/channel/fallback serialization.                                      |
| Workspace integration                | Unified Plugins management panel, activity panels, floating/multiple panel layouts and terminal/connection menu contributions integrate with existing workspace components. Existing workspace and context-menu tests pass.                                                                      |
| Native runtime                       | Real child-process tests exercise handshake identity, persistent reuse, reverse RPC, timeout recovery and crash cleanup. Framed transport tests verify binary round trips and size limits. Activation cancellation and application exit/lock cleanup are implemented.                            |
| Developer workflow                   | UI and native examples both build/package. The extracted native package initializes and returns counts 1 then 2 from the same executable process. UI SDK round-trip tests and browser smoke verify the authoring path.                                                                           |
| Documentation                        | [Plugin guide](plugins.md) specifies installation, trust, package lifecycle, storage, manifest, UI API, native protocol, limitations and maintenance.                                                                                                                                            |

Checks completed:

- `pnpm exec vitest run --maxWorkers=4`: **152 files / 964 tests passed**.
  Default high parallelism exceeded the existing AI settings test's five-second
  timeout; limiting workers passed the entire suite. Temporary reference projects
  under `temp/` are now excluded from test discovery.
- `pnpm build`: frontend type checking, MCP sidecar build and production Vite build
  passed. Existing Browserslist age and large-bundle notices remain.
- `pnpm lint`: passed; one existing dependency warning in `CommandSuggestions.tsx`
  remains. Plugin frontend files have no lint diagnostics.
- Runtime Cargo tests: **9 unit tests and 2 real-process tests passed**.
- Runtime `cargo clippy --all-targets -- -D warnings`: passed.
- Tauri `cargo check --lib` and `cargo clippy --lib`: passed. Existing project-wide
  Clippy warnings remain; plugin-module diagnostics were checked separately.
- Tauri plugin tests: **2 passed**, for atomic lifecycle admission and rendered IPC
  script templates. The Windows library-test binary failed in the loader before
  running any tests; a temporary copy with a Common Controls v6 activation manifest
  ran both successfully. Production application files were not altered for this
  workaround.
- Real Chrome smoke using the actual bridge, SDK and Session Toolbox files:
  panel loading, selected-session request, theme propagation, opaque message origin,
  fixed-scope routing, parent DOM isolation, blocked direct network and absent
  child-frame IPC transport all passed. Native host calls were stubbed in this
  browser check.

This validation does not include a manually operated packaged Tauri desktop build
or macOS/Linux WebView testing. The browser smoke uses HTTP asset serving with the
equivalent sandbox/CSP and mocked native commands; it does not exercise Tauri's
custom resource protocol or real terminal execution dialogs. Verify those in
desktop release QA. Native plugins remain trusted OS processes; the current
format has no cryptographic publisher signatures or marketplace.

## Developer tooling and diagnostics validation

Completed on Windows, 2026-10-03:

- Project creation: UI and Rust starters generated successfully. Three Node tests
  cover external paths (including spaces), valid SDK references, invalid arguments
  and refusal to overwrite existing source.
- Generated UI/Rust packages pass the real package validator. The Rust starter
  builds in release mode. Extracted native-counter **1.0.1** and the generated
  Rust package both complete their SDK handshake, return counts **1, 2** from a
  persistent process, and exit cleanly on stdin EOF.
- Rust SDK: two wire tests cover bounded frames, binary round trips, clean EOF and
  truncated input. Runtime: **10 unit tests + 5 process integration tests** pass
  with `--features sdk-fixture`. SDK process tests cover JSONL/framed transport,
  concurrent and reverse RPC, binary data, structured logs, request cancellation,
  cancellation of timed-out reverse host calls, method errors, panic recovery,
  crashes, and stderr collected during a failing initialization handshake.
- Diagnostic storage tests cover retention limits, UTF-8 truncation, credential
  redaction, stop/restart generations, clearing and removal. The panel distinguishes
  UI-only/disabled plugins and backend states; three frontend tests cover stopping,
  refresh/copy/clear, untrusted log text and stale responses after locking.
- Frontend full suite: **153 files / 967 tests pass**. `pnpm build` and `pnpm lint`
  pass; the existing CommandSuggestions hook warning remains.
- SDK/runtime Clippy with `--all-targets -- -D warnings` passes. Tauri library
  Clippy/check passes; there are no plugin-module diagnostics, while existing
  project-wide warnings remain.
- `pnpm i18n:check` still reports existing ordering issues in the AI text sections
  of all four locales. Plugin sections were formatted independently to preserve
  unrelated content. All four locales contain the same diagnostic keys.

The user reported successful desktop testing of the earlier native-counter
package. The new diagnostics controls have automated frontend coverage; the
updated 1.0.1 package is ready for desktop smoke testing. This does not claim
macOS/Linux validation or a full manual desktop lifecycle test.
