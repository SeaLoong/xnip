//! MCP adapter. Default: edit, apply, peek, find and recover.
//! Specialized line tools and diagnostics are available with `--toolset full`.
//! Write tools share the editing implementation with the CLI.
#![allow(clippy::needless_pass_by_value)]
mod server;
mod tools;
pub use server::{Toolset, serve_stdio, serve_stdio_with_toolset};
