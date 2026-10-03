# NyaTerm plugin development

Read the [plugin guide](../docs/plugins.md) for installation, manifests,
permissions, the UI SDK and native protocol.

- `sdk/nyaterm.js` and `sdk/nyaterm.d.ts`: browser SDK and typings.
- `examples/session-toolbox`: a sandboxed panel using scoped host capabilities.
- `examples/native-counter`: a persistent Rust command backend.

```sh
pnpm plugin:pack plugins/examples/session-toolbox temp/plugins/session-toolbox.nyap
pnpm plugin:example:native
pnpm plugin:pack temp/plugins/native-counter temp/plugins/native-counter.nyap
```

Install the generated packages through the Plugins activity panel and explicitly grant
their requested permissions. Native executables run with OS user permissions.
