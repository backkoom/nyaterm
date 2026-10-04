# Plugin system V1 acceptance

This records evidence for the attached V1 requirements. Automated GPUI rendering
and state tests are separate from native Windows visual/keyboard acceptance.
The latter remains outstanding; the goal is not complete.

## Side-panel change acceptance (2026-10-04)

The plugin side-panel change was exercised on Windows x64 in an isolated portable
instance, using the repository's declarative template example:

* The `MdExtension` entry defaults between Sync / Backup and Settings at the bottom
  left. Opening and closing it keeps the main window alive; the old settings and
  quick-command entry points have been removed.
* Directory-path entry and installation work. The installed enabled plugin survives
  application restart. Inputs support keyboard selection, Tab navigation and Enter
  to generate a preview without sending a command.
* At the minimum 160px sidebar width, text wraps, long buttons stay within the panel
  with ellipsis/full-label tooltips, and the vertical scrollbar stays in the viewport.
* Moving the entry to the right opens the panel on the right. Docked/floating
  switches and panel close/reopen retain parameters and result previews.

These checks cover this UI change; the remaining broader V1 walkthroughs in the
table below are separate.

Validation also passed `cargo check --workspace`, `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets` (no warnings), and
`cargo test --workspace -- --test-threads=1` (3297 passed, 13 existing ignored).
An initial default-parallel workspace run passed; a later run hit the previously
recorded `windows_local_session_close_releases_conpty_reader` timeout. Its exact
rerun and the final serial workspace run passed. No transport code was changed.

## Completion criteria

| Requirement | Implementation and evidence | Remaining native acceptance |
| --- | --- | --- |
| Build application and SDK examples from a clean checkout | Isolated HEAD archive with only plugin changes passed native default build and all workspace checks. SDK guests built independently; examples were componentized, initialized and ZIP packaged. Packaged SDK WIT also compiled. | None for Windows x64 compilation. |
| Start application and open manager | Isolated portable startup created application and plugin databases. The bottom-left extension entry opens a workspace side panel; GPUI tests cover shared service, editing-state retention, placement and docked/floating navigation. | Inspect native layout, keyboard focus, scrolling and close/focus restoration from the extension entry. |
| Install declarative and Wasm examples | Actual manager lifecycle tests install managed directory snapshots and ZIPs; SDK command/text example exports execute. Build script validates all three installable packages. | Install the three examples through the native manager. |
| Invoke, inspect and explicitly fill draft | Actual SDK component runs through the desktop service/panel. GPUI test verifies preview, explicit append, sending remains false, and revoked preview cannot fill. Adapter test preserves session, target, history and saved QuickCommands. | Edit preview, append/replace an existing draft, inspect resulting send box; copy/reuse text. |
| Disable removes actions; enable restores them | Lifecycle test verifies contribution revocation, rejection of disabled calls and fresh instance on re-enable. Shared snapshot drives all windows. | Observe action removal/restoration in both windows. |
| Failed update/reload preserves previous version | Tests execute invalid replacements and actual promoted-directory/preference rollback, then call the retained guest. Windows occupied-file test preserves the old package. | Trigger invalid local update/development reload and invoke the old action. |
| Uninstall leaves no contributions | Lifecycle tests assert package removal, contribution removal, staging cleanup and retained plugin data. UI explicitly says uninstall keeps data. | Uninstall through manager and observe removal in all windows. |
| Restart restores preferences | Real redb round trip/fixture and manager drop/reopen tests preserve enabled/disabled preferences. Corrupt/future preferences are rejected without rewrite. | Restart the isolated portable app and inspect restored states. |
| Fault isolation keeps application/other plugins usable | Real adversarial SDK guest tests hard deadline, fuel, memory, trap, oversized/control results, queue saturation and cancellation; other guest still executes. GPUI only awaits background work. | Observe native responsiveness during a fault and normal close. |
| Repository checks pass | Fresh isolated sources passed build/check/test/fmt/Clippy (3280 passed, 13 existing ignored). Current-source check/fmt/Clippy passed; serial workspace tests passed 3290 with 13 existing ignored. Two default-parallel timing failures and exact passing reruns are recorded in progress. | Native checks above remain separate from Cargo checks. |

## Requirement coverage

* **Contracts and compatibility:** core tests load schema/API V1 fixtures and
  reject unknown fields, unsupported capabilities, incompatible versions,
  duplicate actions and unsafe IDs/paths. Host tests reject duplicate plugin IDs,
  forged ABI markers/interfaces and invalid initialization/input. SDK and host
  generate typed bindings from the same WIT; V1 has no host imports or WASI.
* **Templates and text:** tests exercise bounded substitution and explicit POSIX/
  PowerShell quoting, with no expression execution. Namespaced contributions are
  separate from user commands. SDK guests retain per-instance mutable state;
  separate plugins do not share it. Text input is supplied explicitly and output
  control sequences are rejected before presentation or draft adaptation.
* **Installation safety:** actual directory/ZIP snapshots reject traversal,
  absolute/device/ADS paths, links, duplicate entries and package limits.
  Windows tests cover junctions, file occupation and cleanup of handles. Source
  mutation cannot change an installed snapshot. Development reload uses the same
  validation and preparation transaction as managed packages.
* **Execution and ownership:** bounded guest queues are serial per Store;
  independent guest workers run separately. Infinite execution is actually
  stopped by trapping epoch deadlines/cancellation, with fuel as another budget.
  Tests cover queue-full feedback, cancellation, generation invalidation, reload
  and joined shutdown. DesktopController creates the sole process service;
  closing one workspace does not stop another's guest state. Window entities own
  form/selection/preview state, not a second mutable registry.
* **Data boundaries:** `plugin_preferences_and_packages_stay_out_of_backup_and_cloud_sync`
  uses real Backup/Sync snapshot builders and codecs, `.nya` export/decode and
  native database export/import. Entire exported entity maps are unchanged;
  user QuickCommands restore intact, and the plugin preference table is absent
  from the native backup. Plugin preferences and retained data survive locally.
  Existing credential/encryption/sync/backup formats are not changed.
* **Application integration:** all ordinary fields/buttons/scrolling use
  nyaterm-ui wrappers; six locale catalogs contain the plugin keys. AppShell
  shutdown joins the process service in background work and supports restart
  after an aborted application update. No guest can send terminal bytes, select
  a session, read terminal output or obtain user secrets.
* **Delivery:** independent Apache-2.0 implementation, shared WIT/Rust SDK,
  three installable examples, checked SDK-built Wasm fixtures, build/package/
  verification scripts, design and user/developer documentation. No third-party
  fork changes, commits, pushes or real user configuration changes were made.

## Native verification blocker

The supported computer-use connection fails before window discovery with:

```text
Computer Use native pipe is unavailable: failed to connect native pipe:
系统找不到指定的文件。 (os error 2)
```

It has failed in three consecutive continuation turns, including another
`sky.list_windows()` probe on 2026-10-04. No UI input was sent by these probes.
Kernel reinitialization did not restore the connection. Indirect startup and
GPUI tests do not establish that the native walkthrough above passed.

When the native connection is restored, use a separate portable copy and the
packages produced by `scripts/plugins/build.py --fixtures`; keep real user
configuration untouched. Follow the installation/use steps in
[plugin-system.md](plugin-system.md), then execute the remaining checks in the
table and record their actual outcomes. macOS/Linux and Windows ARM64 runtime
behavior remain unverified; ARM64 host cross-compilation alone passed.
