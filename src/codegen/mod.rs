//! Generates [`library!`](crate::library!) bindings from C headers.
//!
//! Requires the `codegen` feature. This is the engine behind the `dlsym-gen`
//! binary, also usable from a `build.rs`.
//!
//! The pipeline has three stages:
//!
//! 1. [`parse`]: a C front end turns headers into the [`ir`].
//! 2. [`ir`]: a small, language-neutral model of the declarations a header
//!    exposes (functions, types, constants).
//! 3. [`emit`]: turns the IR into Rust source built on
//!    [`library!`](crate::library!).

pub mod emit;
pub mod ir;
pub mod parse;
