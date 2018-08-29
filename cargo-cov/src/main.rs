//! `cargo-cov` is a cargo subcommand which performs source coverage collection and reporting for Rust crates.
//! `cargo-cov` utilizes LLVM's gcov-compatible profile generation pass, and supports a lot of platforms.
//!
//! Please see the [crate README](https://github.com/kennytm/cov#readme) for detail.

#![recursion_limit = "128"] // needed for error_chain.

#![cfg_attr(feature = "cargo-clippy", warn(warnings, clippy_pedantic))]
#![cfg_attr(feature = "cargo-clippy", allow(missing_docs_in_private_items, non_ascii_literal, shadow_reuse, unused_results))]
// `unused_results` caused too many false positive here.

#[macro_use]
extern crate bitflags;
#[macro_use]
extern crate clap;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate serde_json;
extern crate cov;
extern crate env_logger;
extern crate fs_extra;
extern crate fs2;
extern crate glob;
extern crate home;
extern crate md5;
extern crate natord;
extern crate open;
extern crate rand;
extern crate rustc_demangle;
extern crate shell_escape;
extern crate tempfile;
extern crate tera;
extern crate termcolor;
extern crate toml;
extern crate walkdir;

#[macro_use]
mod ui;
mod argparse;
mod cargo;
mod error;
mod lookup;
mod report;
mod shim;
mod sourcepath;
mod template;
mod utils;

use argparse::*;
use cargo::Cargo;
use clap::ArgMatches;
use error::Result;

use std::process::exit;

/// Program entry. Calls [`run()`] and prints any error returned to `stderr`.
///
/// [`run()`]: ./fn.run.html
fn main() {
    if let Err(error) = run() {
        ui::print_error(&error).expect("error while printing error 🤷");
        exit(1);
    }
}

/// Runs the `cargo-cov` program.
fn run() -> Result<()> {
    let matches = parse_args();
    env_logger::init();

    let (subcommand, matches) = matches.subcommand();
    let matches = matches.expect("matches");

    // Forward the shims. Otherwise, ensure it is run as `cargo cov`.
    if subcommand.ends_with(".bat") {
        let forward_args = matches.values_of_os("").unwrap_or_default();
        return match subcommand {
            "rustc-shim.bat" => shim::rustc(forward_args),
            "rustdoc-shim.bat" => shim::rustdoc(forward_args),
            "test-runner.bat" => shim::run(forward_args),
            _ => panic!("Don't know how to run {}", subcommand),
        };
    } else if subcommand != "cov" {
        panic!("This command should be executed as `cargo cov`.");
    }
    debug!("matches = {:?}", matches);

    // Read the --profiler/--target/--manifest-path options specified before the subcommand:
    //
    //     cargo cov --manifest-path Cargo.toml clean ...
    //               ^~~~~~~~~~~~~~~~~~~~~~~~~~
    let mut special_args = SpecialMap::with_capacity(3);
    update_from_clap(matches, &mut special_args);

    // Read the --profiler/--target/--manifest-path options specified after the subcommand:
    //
    //     cargo cov clean --manifest-path Cargo.toml ...
    //                     ^~~~~~~~~~~~~~~~~~~~~~~~~~
    let (subcommand, matches) = matches.subcommand();
    let matches = matches.expect("matches");
    update_from_clap(matches, &mut special_args);

    // Extracting --profiler/--target/--manifest-path if they are written in an external subcommand (build, test, run).
    let forward_args = match matches.values_of_os("") {
        Some(args) => normalize(args, &mut special_args),
        None => Vec::new(),
    };
    let cargo = Cargo::new(special_args, forward_args);

    // Actually run the subcommands. Please do not pass ArgMatches as a whole to the receiver functions.
    match subcommand {
        "build" | "test" | "run" => cargo?.forward(subcommand)?,
        "clean" => clean(&cargo?, matches)?,
        "report" => generate_reports(cargo, matches)?,
        _ => ui::print_unknown_subcommand(subcommand)?,
    }

    Ok(())
}

/// Parses the command line arguments using `clap`.
fn parse_args() -> ArgMatches<'static> {
    const HELP_TEMPLATE: &str = "\
{about}

Usage:
    cargo cov <subcommand> [options]

Options:
{options}

Subcommands:
    build     Compile the crate and produce coverage data (*.gcno)
    test      Test the crate and produce profile data (*.gcda)
    run       Run a program and produces profile data (*.gcda)
{subcommands}
";

    clap_app!(cargo =>
        (bin_name: "cargo")
        (@setting AllowExternalSubcommands)
        (@subcommand cov =>
            (author: crate_authors!(", "))
            (about: crate_description!())
            (version: crate_version!())
            (template: HELP_TEMPLATE)
            (@setting DeriveDisplayOrder)
            (@setting ArgRequiredElseHelp)
            (@setting GlobalVersion)
            (@setting AllowExternalSubcommands)
            (@arg profiler: --profiler [LIB] +global "Path to `libclang_rt.profile_*.a`")
            (@arg target: --target [TRIPLE] +global "Target triple which the covered program will run in")
            (@arg ("manifest-path"): --("manifest-path") [PATH] +global "Path to the manifest of the package")
            (@subcommand clean =>
                (about: "Clean coverage artifacts")
                (@setting UnifiedHelpMessage)
                (@group build =>
                    (@arg gcda: --gcda "Remove the profile data only (*.gcda)")
                    (@arg local: --local "Remove the build artifacts in current workspace")
                    (@arg all_crates: --("all-crates") "Remove build artifacts of all crates")
                )
                (@arg report: --report "Remove the coverage report")
            )
            (@subcommand report =>
                (about: "Generates a coverage report")
                (@arg template: --template [TEMPLATE] "Report template, default to 'html'")
                (@arg open: --open "Open the report in browser after it is generated")
                (@arg include: --include [TYPES]... +use_delimiter possible_values(&[
                    "local",
                    "macros",
                    "rustsrc",
                    "crates",
                    "unknown",
                    "all",
                ]) "Generate reports for some specific sources")
                (@arg workspace: --workspace [PATH] "The directory to find the source code, default to the current Cargo workspace")
                (@arg output: --output -o [PATH] "The directory to store the generated report, default to `<src>/target/cov/report/`")
                (@arg gcno: --gcno [PATH] "The directory that contains all *.gcno files, default to `<src>/target/cov/build/gcno/`")
                (@arg gcda: --gcda [PATH] "The directory that contains all *.gcda files, default to `<src>/target/cov/build/gcda/`")
            )
        )
    ).get_matches()
}

/// Parses the command line arguments and forwards to [`report::generate()`].
///
/// [`report::generate()`]: report/fn.generate.html
fn generate_reports(cargo: Result<Cargo>, matches: &ArgMatches) -> Result<()> {
    let report_config = ReportConfig::parse(matches, cargo.map(Cargo::into_cov_build_path))?;
    let open_path = report::generate(&report_config)?;
    if matches.is_present("open") {
        if let Some(path) = open_path {
            progress!("Opening", "{}", path.display());
            let status = open::that(path)?;
            if !status.success() {
                warning!("failed to open report, result: {}", status);
            }
        } else {
            warning!("nothing to open");
        }
    }

    Ok(())
}

/// Parses the command line arguments and forwards to [`Cargo::clean()`].
///
/// [`Cargo::clean()`]: cargo/struct.Cargo.html#method.clean
fn clean(cargo: &Cargo, matches: &ArgMatches) -> Result<()> {
    use cargo::*;

    let mut clean_target = CleanTargets::empty();

    if matches.is_present("gcda") {
        clean_target |= CleanTargets::BUILD_GCDA;
    } else if matches.is_present("local") {
        clean_target |= CleanTargets::BUILD_GCDA | CleanTargets::BUILD_GCNO;
    } else if matches.is_present("all_crates") {
        clean_target |= CleanTargets::BUILD_EXTERNAL | CleanTargets::BUILD_GCDA | CleanTargets::BUILD_GCNO;
    }
    if matches.is_present("report") {
        clean_target |= CleanTargets::REPORT;
    }

    if clean_target.is_empty() {
        clean_target = CleanTargets::BUILD_GCDA | CleanTargets::BUILD_GCNO;
    }
    cargo.clean(clean_target)
}
