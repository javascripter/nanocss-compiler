# NanoCSS Compiler Rust Workspace

Rust crates for NanoCSS Compiler integrations live here.

## Crates

- `crates/nanocss_swc`: Rust compiler and SWC transform plugin crate.
- `crates/nanocss_node`: Node native binding used by `nanocss/transform` and
  the PostCSS extractor.

## Commands

```sh
cargo check
cargo test
cargo build -p nanocss_swc --target wasm32-wasip1 --release
cargo build -p nanocss_node --release
```

The root package build copies the release wasm artifact to `dist/swc.wasm`,
which is exposed as `nanocss-compiler/swc` for integrations that accept SWC wasm plugins.
It also copies the Node native binding to `dist/nanocss_node.node`, which is
loaded by `nanocss-compiler/postcss-plugin`.
