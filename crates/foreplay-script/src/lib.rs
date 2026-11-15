//! Dual scripting host: Native Rust hot-reload + zero-compile `rustports-quickjs` (TS/JS).

/// Supported scripting frontends compiling to / operating over `foreplay-ir`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ScriptFrontend {
    NativeRustCrate,
    QuickJsTypeScript,
    GdScriptCompat,
}
