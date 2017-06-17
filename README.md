`cargo-cov`: Source coverage for Rust
=====================================

`cargo-cov` is a cargo subcommand which performs source coverage collection and reporting for Rust crates. `cargo-cov`
utilizes LLVM's gcov-compatible profile generation pass, and supports a lot of platforms.

* ✓ Linux, Windows (MSVC only), macOS
* ✓ x86_64, x86
* ✓ Rust 1.17 — 1.20

Usage: for Local Testing
------------------------

You may install `cargo-cov` via `cargo`. Rust 1.17.0 or above is required.

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



<!--

Version requirement:

    - Rust 1.13.0: needed for `?`
    - Rust 1.15.1: needed for proc-macro (for serde)
    - Rust 1.17.0: needed for BTreeMap::range

-->