# Plugin system V1 design

NyaTerm implements this system independently under Apache-2.0. Zed's extension
manifest, proxy, builder, guest API/WIT, store, Wasm host, capability checker,
headless host and lifecycle tests were inspected as architectural references;
their GPL implementation is not copied or linked.

## Boundaries and ownership

* `nyaterm-core::plugins` owns strict manifest/preferences contracts, identifiers,
  compatibility, parameters, bounded template expansion and result validation.
* `nyaterm-plugin-api` owns the single versioned WIT and Rust guest registration.
* `nyaterm-plugin-host` owns managed snapshots, installation transactions and
  the authoritative registry. Each component has one worker, Store and serial
  bounded queue. Different components run independently.
* `nyaterm-store::plugin_preferences` persists an isolated versioned document
  under AppRuntime's plugin directory. Existing redb, sync and backup contracts
  are untouched. Corrupt preferences fail closed and are never overwritten.
* DesktopController owns one process service. GPUI windows own input entities,
  action selection and previews; immutable snapshots are presentation only.
  A settings entry opens the manager/action surface. All host work runs off UI.

## Interface and execution

Schema 1, semantic plugin versions and API 1.0.0 are distinct. WIT exports
version, initialize, invoke and shutdown; arguments/results/errors are typed.
Guest binaries carry a custom API section. Both the section and actual typed
component exports are checked. V1 exposes no host imports, WASI environment,
filesystem, network, process, clipboard, credentials or terminal access.
Unsupported capability declarations are rejected.

Wasmtime 39.0.1 uses synchronous component calls on dedicated workers. Fuel is
a finite CPU budget; an epoch ticker interrupts deadlines and cancellation with
a trap (never async yield). Store limits cap guest memory and instance counts.
All inputs/results/queues/packages are bounded. Cancellation invalidates a
generation before stopping its worker. Old results cannot revive contributions.
Trap/timeout/invalid results retire that instance. Shutdown joins workers off UI.

## Installation and persistence

Directories and ZIP packages are copied into staging, rejecting links, reparse
points, traversal, Windows device/ADS paths, duplicates and package limits.
Validation, component compilation, instantiation and initialization happen on
the snapshot before promotion. Updates/reloads prepare the replacement before
switching; failure leaves the old instance and contributions intact. File bytes
are read before compilation, so component execution holds no package file
handles. Directory rename rollback and preference transactions protect Windows
replacement. Development sources also become validated snapshots.

Layout: `data_dir/plugins/{installed,staging,work/data,dev}` plus isolated
preferences. The installed tree is the rebuildable catalog; preferences carry
enabled state and development source, not contributions. Uninstall retains
plugin data, explicitly described in the UI and user documentation.

## Product behavior

Templates and component actions share namespaced contributions; they never
modify QuickCommandsConfig. Parameter substitution supports only `${name}`
and explicit shell quoting rules, without script evaluation. Actions receive
only user-supplied values/text. Results are ordinary text, with controls rejected.
Command results are editable previews. Filling the send box is explicit and
preserves an existing draft using append or an explicit replacement action.
No plugin operation selects sessions, writes terminal bytes or presses Enter.

## Verification

Core/store contract tests, actual SDK-built components and host lifecycle tests
cover installation, rollback, isolation, failures, cancellation, generations,
preferences and cleanup. Desktop tests verify process sharing and draft-only
adaptation. Scripts build examples/fixtures and validate packages. Windows is
the current platform; other platforms must be listed as unverified until tested.

The isolated Windows x64 source snapshot passed the default build and all four
workspace checks with a fresh target cache: 3280 tests passed with 13 pre-existing
ignored tests. SDK packaging and Windows ARM64 host cross-compilation passed.
Native startup was smoke-tested using isolated portable data; computer-use's
native pipe was unavailable, so visual/keyboard interaction remains unverified.
ARM64 execution and Linux/macOS behavior are also unverified.
