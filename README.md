`cargo-cov`: Source coverage for Rust
=====================================

`cargo-cov` is a cargo subcommand which performs source coverage collection and reporting for Rust crates. `cargo-cov`
utilizes LLVM's gcov-compatible profile generation pass, and supports a lot of platforms.

* ✓ Linux, Windows (MSVC only), macOS
* ✓ x86_64, x86
* ✓ Rust 1.17 — 1.20

Usage: for Local Testing
------------------------

You may install `cargo-cov` via `cargo`. Rust 1.19.0 or above is recommended.

```sh
cargo install cargo-cov
```

The typical workflow is like this:

```sh
# clean up previous coverage result
cargo cov clean

# test the code
cargo cov test

# open the coverage report
cargo cov report --open
```

**Warning:** Using `cargo cov test` before 1.19 will produce a corrupt report due to lack of
[target-specific runner](https://github.com/rust-lang/cargo/pull/3954) that can prevent the program from trying to merge
two incompatible coverage data analysis. If you must use pre-1.19 toolchain, please execute the doctests *before* the
normal tests:

```sh
# Run --doc tests before other things before 1.19
cargo cov test --doc
cargo cov test --lib
```

<!--

Version requirement:

    - Rust 1.13.0: needed for `?`
    - Rust 1.15.1: needed for proc-macro (for serde)
    - Rust 1.17.0: needed for BTreeMap::range

-->