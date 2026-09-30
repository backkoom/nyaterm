# NyaTerm vnc-rs fork

This directory vendors `vnc-rs` 0.5.3 from
<https://github.com/HsuJv/vnc-rs> at commit
`ab684d009d767c968af2f7559576334038623124`.

The upstream crate is dual licensed under MIT or Apache-2.0. The original
`LICENSE-MIT`, `LICENSE-APACHE`, Cargo package authors, and source-level
attribution are preserved.

NyaTerm vendors this crate to harden its network parser before application
integration. Local changes forbid unsafe Rust, replace network-controlled panic
and undefined-behavior paths with typed errors, add bounded protocol limits,
reduce queue sizes, add explicit security-selection policy, and add
deterministic handshake/parser regression tests. The fork also implements
RA2_256 (security type 129) with RSA key exchange, AES-EAX records, bounded
credentials, and an application-provided server-key verifier that runs before
credentials are sent.

This fork is used by NyaTerm's direct-TCP VNC manager and React pane. Raw must
remain the required fallback. ZRLE/Tight support should only be advertised after
the corresponding decoder hardening tests and interoperability checks pass.

## Refresh procedure

1. Fetch the exact upstream revision recorded above.
2. Preserve both license files and upstream attribution.
3. Reapply the smallest reviewed hardening diff.
4. Run `cargo fmt --check`, `cargo check`, and debug/release `cargo test` using
   this manifest, then run the root Tauri Cargo check.
5. Update the revision and local-change notes here and in `../README.md`.
