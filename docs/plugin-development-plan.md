# Plugin development and diagnostics

This stage completes four deliverables on top of the local plugin host:

Implemented and verified. See [the development guide](plugin-development.md),
[native SDK reference](../plugins/sdk/rust/nyaterm-plugin-sdk/README.md) and
[validation report](plugin-system-validation.md#developer-tooling-and-diagnostics-validation).

- `pnpm plugin:create <id> <directory> --template ui|rust` creates a new, buildable
  project without overwriting existing content. Each template includes a manifest,
  working entrypoint, SDK integration, build/package instructions and README.
- A standalone Rust native SDK handles identity/API handshake, JSONL and framed
  transport, concurrent requests, scoped reverse host calls, structured errors,
  timeouts, cooperative cancellation and logging. The existing native counter
  and generated Rust projects use the SDK.
- The Plugins panel displays idle/starting/running/stopped/error states for native
  backends and distinguishes UI-only and disabled plugins. Users can stop a backend
  and see its next invocation activate a fresh process.
- Users can inspect, refresh, copy and clear bounded in-memory logs. Startup,
  protocol, crash and invocation errors are retained even during the handshake.
  Old processes cannot overwrite the state of a newer activation. Logs are
  treated as untrusted text, redact host-known secrets/scope tokens, and are not
  persisted or added to backup/cloud sync.

Verification must exercise generated projects through build/package validation,
the SDK through the real host runtime in both transports (including reverse RPC,
errors, cancellation and crash handling), status/log transitions and retention,
and the frontend diagnostic controls. Type checking, relevant lint and frontend
build remain final gates. Marketplace, connection provider APIs and browser data
stream subscriptions remain separate stages.
