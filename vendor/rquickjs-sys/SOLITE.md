# Vendored rquickjs-sys 0.14.0 (Solite, spec 041)

`rquickjs-sys` 0.14.0 as published on crates.io (MIT, QuickJS-ng 0.16.2: `quickjs/LICENSE`), with
three changes:

- `quickjs/quickjs.c`: `JS_ARENA_LARGE_BLOCKS_ONLY` is no longer an unconditional `#define` (the
  small-object arena costs about 85 KB per runtime); an `#ifdef` lets the build set it.
- `build.rs`: defines `JS_ARENA_LARGE_BLOCKS_ONLY=1` (no environment variable needed).
- `quickjs/quickjs.c`: `JS_NO_FUNCTION_SOURCE` (defined by `build.rs`) makes the compiler keep no
  copy of a function's source text at its three sites (function, arrow/method body, class
  constructor). Each function otherwise holds its own source, nested functions included: about
  15 KB for the script shim alone in every runtime. `Function.prototype.toString` then answers
  `function name() { [native code] }`; nothing else reads the text.

Dropped, not needed to build: `quickjs/` tests, docs, examples, generators and the command-line
programs; `vendor/wasi-libc`, `wasm-shim`; the bundled bindings for `wasm32-*` and Windows targets.
The root `Cargo.toml` and `firmware/Cargo.toml` patch crates.io's crate with this one.
