`cargo-cov`: Source coverage for Rust
=====================================

`cargo-cov` is a cargo subcommand which performs source coverage collection and reporting for Rust crates. `cargo-cov`
utilizes LLVM's gcov-compatible profile generation pass, and supports a lot of platforms.

* ✓ FreeBSD, Linux, macOS, Windows (MSVC only)
* ✓ x86_64, x86

Usage: for Local Testing on nightly Rust
----------------------------------------

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

Usage: for Testing on stable Rust (1.19+)
-----------------------------------------

We strongly recommend you use nightly Rust since only the nightly toolchain has built-in instrumented profiling support
via [`-Zprofile`](https://github.com/rust-lang/rust/issues/42524).

If you must use a stable toolchain, you may try the following:

1. Install the compiler-rt profile library.

    | Target         | Instruction                                                  |
    |:---------------|:-------------------------------------------------------------|
    | Ubuntu, Debian | Install `libclang-common-6.0-dev`, or simply install `clang` |
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

We do not guarantee that a correct coverage profile will be generated using this method.

[Clang for Windows]: http://releases.llvm.org/download.html
