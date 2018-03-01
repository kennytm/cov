`cargo-cov`: Source coverage for Rust
=====================================

`cargo-cov` is a cargo subcommand which performs source coverage collection and reporting for Rust crates. `cargo-cov`
utilizes LLVM's gcov-compatible profile generation pass, and supports a lot of platforms.

* ✓ FreeBSD, Linux, macOS, Windows (MSVC only)
* ✓ x86_64, x86
* ✓ Rust 1.17 — 1.20

Usage: for Local Testing on Rust 1.19+
--------------------------------------

You may install `cargo-cov` via `cargo`.

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

Usage: for Testing on Rust 1.17 to 1.18
---------------------------------------

We strongly recommend you use Rust 1.19 or above since the following features will greatly improve coverage quality:

* [Official instrumented profiling support via `-Zprofile`](https://github.com/rust-lang/rust/issues/42524).
* [Configurating target-specific runner in Cargo](https://github.com/rust-lang/cargo/pull/3954).
* [Listing `target/` directory in `cargo metadata`](https://github.com/rust-lang/cargo/pull/4022).

In particular, target-specific runner is required to prevent the profiled program from trying to merge two incompatible
coverage data analysis which will corrupt the coverage report. If you must use pre-1.19 toolchain, please do the
following:

1. Install the compiler-rt profile library.

    | Target         | Instruction                                                  |
    |:---------------|:-------------------------------------------------------------|
    | Ubuntu, Debian | Install `libclang-common-3.8-dev`, or simply install `clang` |
    | Fedora         | Install `compiler-rt`                                        |
    | OpenSUSE       | Install `llvm-clang`                                         |
    | Windows (MSVC) | Install [Clang for Windows] Pre-Built Binary from LLVM       |
    | macOS, iOS     | Provided by the Xcode command line tools                     |
    | Android        | Provided by Android NDK                                      |

2. Execute the doc-test *separately* from the normal tests. Run the doc-test *before* the normal tests.

    ```sh
    # Run --doc tests before other things before 1.19
    cargo cov test --doc
    cargo cov test --lib
    ```

(`cargo-cov` does not support Rust 1.16 or below since it uses [`BTreeMap::range`] which is stablized since 1.17.)

[Clang for Windows]: http://releases.llvm.org/download.html
[`BTreeMap::range`]: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.range
