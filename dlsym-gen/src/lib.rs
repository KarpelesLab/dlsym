//! Generates [`dlsym`] bindings from C headers.
//!
//! The pipeline has three stages:
//!
//! 1. [`parse`]: a C front end turns headers into the [`ir`].
//! 2. [`ir`]: a small, language-neutral model of the declarations a header
//!    exposes (functions, types, constants).
//! 3. [`emit`]: turns the IR into Rust source built on [`dlsym::library!`].
//!
//! This crate is usable both as the `dlsym-gen` binary and as a library, e.g.
//! from a `build.rs`.

pub mod emit;
pub mod ir;
pub mod parse;
