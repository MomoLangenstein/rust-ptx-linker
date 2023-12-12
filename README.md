# Rust Embedded Linker
The rust embedded linker can be used to link targets embedded (no dependency on system libraries) targets in llvm-bc before compiling to native code. For some of these targets (e.g. ptx) there does not exist a sensible way to link the native format at all, this means that the embedded-linker (or similar alternatives) will be the preferred way for rustc to link when building for these targets.

### Relation to ptx and rust-ptx-linker
The main reason for creating this tool was being able to link ptx after the `rust-ptx-linker` project was abandoned.

### Development
This crate is taken in slightly modified from from [rust#117458](https://github.com/rust-lang/rust/pull/117458) to provide a drop-in replacement for the original `rust-ptx-linker` until the embedded linker developed in this PR is made available. All credit goes to [@kjetilkjeka](https://github.com/kjetilkjeka).
