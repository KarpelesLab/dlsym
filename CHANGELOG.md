# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.0.2](https://github.com/KarpelesLab/dlsym/compare/v0.0.1...v0.0.2) - 2026-10-22

### Added

- *(codegen)* probe the library for missing symbols; follow in-library includes
- *(codegen)* libclang front end, full emitter and crate generation
- [**breaking**] loader extensions and extern-block syntax for library!

### Fixed

- *(codegen)* never unload libclang; add Builder and --safe

### Other

- add symbols and bindings examples
