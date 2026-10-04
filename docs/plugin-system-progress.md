# Plugin system implementation progress

Goal: complete all requirements in the attached plugin-system V1 specification.
Do not mark complete until the documented acceptance checks have passed.

## Completed

* Read root AGENTS.md; no nested AGENTS.md found by tracked file search.
* Initial `git status --short` was empty; preserve subsequent unrelated edits.
* Inspected required Zed reference modules (read-only), runtime directories,
  DesktopController/AppShell multiwindow shutdown, settings and command composer.
* Wrote independent architecture decisions in plugin-system-design.md.
* Verified toolchain: Rust 1.97.1, Windows MSVC x64 and ARM64 targets installed.
* Inspected crate metadata: Wasmtime 39.0.1 MSRV 1.89, wit-bindgen 0.49 MSRV 1.82.
* Implemented strict core contracts, isolated redb preference store, shared WIT
  and Rust SDK, three examples plus a stateful adversarial SDK guest.
* Implemented managed directory/ZIP snapshots, transactional promotion/rollback,
  lifecycle registry, per-component serial worker, bounded queues, fuel and
  epoch hard cancellation/deadlines, resource limits and generation leases.
* Added process-owned service to DesktopController and orderly shutdown paths.
  Manager UI is reachable from Settings > General and the quick-command toolbar.
  It uses NyaInput/NyaButton/NyaScrollable and all six locale catalogs.
* SDK guests actually build for wasm32-unknown-unknown. Build script componentizes,
  validates and packages examples; checked SDK-generated binary fixtures support
  ordinary clean-checkout tests without needing the guest toolchain first.

## Checks run (Windows x64)

* `cargo check -p nyaterm-plugin-api -p nyaterm-core`: passed.
* `python scripts/plugins/build.py --fixtures`: passed; all three packages
  validated including real Wasmtime instantiation/initialize of both Wasm examples.
* `cargo test -p nyaterm-core --test plugins`: 4 passed.
* `cargo test -p nyaterm-store plugin_preferences`: 2 passed.
* `cargo test -p nyaterm-plugin-host`: 10 lifecycle tests passed (3.05s).
* `cargo check -p nyaterm-app`: passed after UI integration.
* `cargo check --workspace`: passed (61s).
* Expanded host tests: 13 passed, including real promoted-directory rollback,
  Windows deny-delete file occupation and junction rejection.
* `cargo test -p nyaterm-desktop features::plugins`: 3 passed, including actual
  NyaTermApp draft-only adaptation and controller-owned shared component state.
* Added and passed a fourth GPUI test that renders the manager, calls the actual
  SDK component, previews/edits results and verifies explicit fill and lease
  invalidation before the catalog event reaches the view.
* `cargo check -p nyaterm-plugin-host --target aarch64-pc-windows-msvc`: passed;
  this is compilation only, not ARM64 runtime verification.
* Dependency metadata reviewed for both workspaces: added Wasmtime/Cranelift MSRV
  1.89, wit-bindgen 1.82, wasmparser/wit-component 1.76, TOML 1.76; no desktop
  linkage in guest dependency graph. Packaged SDK verification passed.
* `python scripts/plugins/verify.py --workspace` ran sequentially:
  clean guest rebuild/pack validation, 4 core + 2 store + 13 host + 4 GPUI tests
  all passed. Default-member native build and workspace check also passed.
* `cargo fmt --all -- --check`, guest-workspace fmt check and Python script
  compilation passed. Earlier Clippy pass found two host style suggestions;
  both were fixed before the final verification run.
* `cargo build --locked`: passed, built application and all three runtime helpers.
* Final `cargo check --locked --workspace`: passed.
* `cargo package -p nyaterm-plugin-api --allow-dirty`: passed including verification
  of the SDK extracted under target/package (WIT included and resolved correctly).
* Native smoke-start used copied binaries under `target/plugin-ui-smoke` with a
  portable marker. Process stayed responsive and created its isolated app database
  and `data/plugins/dev/preferences.redb`, proving DesktopController started the
  process plugin service. The test process was stopped; no real config was used.
  Hidden smoke process had no discoverable MainWindowHandle, so this is startup
  evidence only, not a visual or keyboard acceptance claim.
* First full workspace test run reached 1712 passed desktop tests but failed two
  i18n catalog-parity checks: four existing locales lacked the new plugin keys.
  This was introduced here (not baseline). Added complete plugin translations to
  zh-TW, ja, ko and fr, keeping all prior catalog content unchanged. Targeted i18n
  tests passed (16); the final desktop suite passed all 1714 tests, with 7 existing
  ignored tests.
* Initial full Clippy run finished with exit 0 after both host suggestions were
  fixed. Full check logs and structured results live under target/plugin-verification.
* `cargo package -p nyaterm-plugin-api --allow-dirty --no-verify`: packaged the
  SDK and WIT successfully (no publication). Standalone guest workspace builds.
* Applied intentional rustfmt to new code and modified Rust files.

## Final verification and concurrent workspace edits

The final sequential build/check/test/fmt/Clippy run is recorded in
`target/plugin-verification/final-results.json` and `final-*.log`.

* Build and workspace check: exit 0.
* Workspace unit/integration suites: 3254 passed, no failed assertions, 12
  existing ignored tests. The command exited 1 at desktop doctests: while it ran,
  another task changed terminal editing and introduced an import of
  `command_starts_line_editor` after the core library had already compiled.
  The function now exists in the concurrently modified core source. This is a
  changing-workspace compilation mismatch, not an established baseline failure.
* Formatting: exit 1, with differences only in the concurrently modified
  `command_suggestion_suppression.rs`, `command_suggestions/mod.rs`,
  `editing_runtime.rs` and `editing_state.rs`. These files are not plugin work;
  their changes are preserved without overwriting or formatting them here.
* Clippy across the workspace and all targets: exit 0.

The follow-up workspace check/test/fmt run passed all three commands (exit 0),
recorded separately in `target/plugin-verification/recheck-results.json` and
`recheck.log`, `retest.log`, `refmt.log`. Workspace tests completed including
doctests: 3283 passed, 0 failed, 13 ignored. These results include concurrent
terminal changes; that task resolved its formatting/import issues independently.

Because the unrelated terminal work continued changing during verification,
created an isolated HEAD archive and overlaid only the plugin sources and their
integration edits. `target/plugin-verification/clean-checkout.json` records the
base revision and exact overlay list; `clean-results.json` and `clean-*.log`
record its build/check/test/fmt/Clippy checks. The shared E: target attempt hit
MSVC LNK1140 while linking the application, then OS error 112 (insufficient disk
space) writing an incremental query cache; tests also reported LLVM output
failure from the full disk. These are recorded failures, not successful checks
or an assumed baseline defect. A fresh target on D: successfully built the same
isolated sources and passed workspace check, tests, formatting and Clippy.
All five commands returned exit 0. Results are in `fresh-results.json` /
`fresh-*.log` and its
explicit target directory in `fresh-target.json`. Its guest workspace was built
from source with an independent target directory: all SDK examples and the
adversarial fixture compiled. Componentization and real initialization/package
validation passed for all three installable packages, recorded in
`clean-guests-result.json` and `clean-packages-results.json`.
When E: became full, moved only this task's completed `clean-guest-target` cache
to the fresh D: target's `saved-guest-cache` directory, freeing 211 MB for logs
and documentation. Guest source, fixtures and packaged examples remain in place.
Also verified no smoke application was running and moved this task's isolated
`target/plugin-ui-smoke` directory to `saved-ui-smoke` under the D: verification
target. This retained its startup evidence and left about 457 MB free on E:.
The original workspace and its concurrent terminal changes remain untouched.

Isolated workspace tests: 3280 passed, 0 failed, 13 existing ignored tests across
45 suites including doctests. The three additional passing tests in the main
workspace belong to concurrent terminal work, not this plugin implementation.
No new plugin tests are ignored. All 62 overlaid code/manifest/fixture/script
files were compared to the main workspace and matched exactly.

The final Python-only change resolves both native-tool and guest target paths
through Cargo metadata, so `scripts/plugins/build.py` honors `CARGO_TARGET_DIR`
and Cargo target-directory configuration. A complete examples/fixtures build
using the fresh D: target passed (exit 0); results are recorded in
`target-override-build-result.json` and `target-override-build.log`. All three
rebuilt SDK component fixtures matched their previous SHA-256 hashes exactly.
Python compilation and Git whitespace checks passed. At that point no Rust or
locale sources had changed after the successful isolated checks. The subsequent
acceptance audit added the storage regression test documented below.

## Acceptance audit follow-up (2026-10-04)

Added `plugin_preferences_and_packages_stay_out_of_backup_and_cloud_sync` in
`nyaterm-store/src/plugin_preferences.rs`. It exercises actual Backup/Sync
snapshot builders and codecs, `.nya` export/decode and native database
export/import, comparing complete entity maps and preserved QuickCommands.
Plugin preferences/resources/data/staging stay outside those exports; local
plugin preferences and data remain intact. No production behavior changed.
`cargo test --locked -p nyaterm-store plugin_preferences`: 3 passed, recorded in
`acceptance-store-test.log` and `acceptance-store-test-result.json`.

The first current-source broad check/test/Clippy attempt reused the isolated
archive's target directory and incorrectly reused its older core artifacts:
terminal APIs present in current source were missing from the cached library.
Formatting passed. The exit-101 failures remain recorded in
`acceptance-results.json` and `acceptance-*.log`; they are not a baseline defect.
Reversibly moved only the task-owned target's 47 core cache entries under an
exclusive Cargo lock to `stale-core-cache-20261004`, then reran the four checks.
Current results are recorded in `acceptance-recheck-results.json` and
`acceptance-recheck-*.log`.

Check, formatting and Clippy all returned exit 0 after cache invalidation.
The workspace test run passed all plugin/core/store/desktop suites, but failed
`tests::windows_local_session_close_releases_conpty_reader` in the transport
suite on its five-second close deadline (376 passed, 1 failed in that suite).
That test/source is unchanged from HEAD and passed in both `fresh-test.log`
and `retest.log`; this single failure alone does not establish a baseline defect.
The failure is preserved in `acceptance-recheck-test.log`. A targeted rerun and
another complete workspace test run are recorded separately in
`acceptance-test-retry-results.json`, `acceptance-conpty-retry.log` and
`acceptance-workspace-retry.log`; no assertion or transport source was changed.

The exact ConPTY rerun passed (1 test, 0.02 seconds). The next default-parallel
workspace run instead failed the unchanged
`http::ai::proxy::tests::custom_bypass_and_direct_reach_the_origin_without_a_proxy`
on Windows socket `WouldBlock` during its two-second request read; its previous
run passed. Its exact rerun also passed. Both failures remain visible in the
logs. A full serial run (`cargo test --workspace --locked -- --test-threads=1`)
is recorded in `acceptance-workspace-serial.log` and
`acceptance-serial-results.json`. No existing tests were ignored, relaxed or
removed to handle these failures.

The serial workspace run returned exit 0: 3290 passed, 0 failed, 13 existing
ignored tests across 45 suites including doctests (140.5 seconds). This uses
current HEAD `492d22d2707336f2adbf9441c19c3c77ff9bae96`, including the separately
committed terminal work, plus the plugin implementation and new storage test.
Current workspace check, formatting and all-targets Clippy returned exit 0.
Default-parallel test timing failures are retained above, not presented as an
unqualified parallel pass. Git whitespace validation also passed.

[plugin-system-acceptance.md](plugin-system-acceptance.md) maps every completion
criterion and test requirement to actual evidence and identifies the remaining
native walkthrough. Another supported `sky.list_windows()` probe failed with
the same native-pipe OS error 2 before discovering any windows. No UI input was
sent. This blocker has persisted through three consecutive continuation turns;
native visual/keyboard acceptance remains outstanding.

All independent implementation, compatibility regression and automated
verification work is finished. Goal is blocked on restoring the supported native
computer-use connection for the remaining visual/keyboard walkthrough; it is
not complete. Resume using the acceptance table once that connection works.

## Checks and limitations

Live Windows UI capture is unavailable: computer-use list_windows
failed with native-pipe OS error 2 twice and again after kernel reinitialization.
No live visual/keyboard acceptance is claimed. GPUI tests remain available.
No source copied from GPL reference, no temp/vendor edits, no real user data
changes, no commits or pushes. Native visual/keyboard acceptance remains
outstanding; Goal is not marked complete.
