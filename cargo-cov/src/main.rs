//! `cargo-cov` is a cargo subcommand which performs source coverage collection and reporting for Rust crates.
//! `cargo-cov` utilizes LLVM's gcov-compatible profile generation pass, and supports a lot of platforms.
//!
//! Please see the [crate README](https://github.com/kennytm/cov#readme) for detail.

#![doc(html_root_url="https://docs.rs/cargo-cov/0.1.0")]
#![cfg_attr(feature="cargo-clippy", warn(anonymous_parameters, fat_ptr_transmutes, missing_copy_implementations, missing_debug_implementations, missing_docs, trivial_casts, trivial_numeric_casts, unsafe_code, unused_extern_crates, unused_import_braces, unused_qualifications, variant_size_differences))]
#![cfg_attr(feature="cargo-clippy", warn(filter_map, items_after_statements, mut_mut, mutex_integer, nonminimal_bool, option_map_unwrap_or, option_map_unwrap_or_else, option_unwrap_used, print_stdout, result_unwrap_used, similar_names, single_match_else, wrong_pub_self_convention))]
// Note: NOT enabling the `unused_results` lint, too many false positive here.

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
extern crate copy_dir;
extern crate cov;
extern crate either;
extern crate env_logger;
extern crate glob;
extern crate md5;
extern crate natord;
extern crate open;
extern crate rand;
extern crate rustc_demangle;
extern crate shell_escape;
extern crate tempdir;
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
use either::Either;
use error::Result;
use sourcepath::*;

use std::ffi::OsStr;
use std::iter::empty;
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
    env_logger::init().expect("initialized logger");

    let (subcommand, matches) = matches.subcommand();
    let matches = matches.expect("matches");

    // Forward the shims. Otherwise, ensure it is run as `cargo cov`.
    if subcommand.ends_with(".bat") {
        let forward_args = match matches.values_of_os("") {
            Some(a) => Either::Left(a),
            None => Either::Right(empty()),
        };
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
    let cargo = Cargo::new(special_args, forward_args)?;

    // Actually run the subcommands. Please do not pass ArgMatches as a whole to the receiver functions.
    match subcommand {
        "build" | "test" | "run" => {
            cargo.forward(subcommand)?;
        },
        "clean" => {
            let gcda_only = matches.is_present("gcda_only");
            let report = matches.is_present("report");
            cargo.clean(gcda_only, report)?;
        },
        "report" => {
            generate_reports(&cargo, matches)?;
        },
        _ => {
            ui::print_unknown_subcommand(subcommand)?;
        },
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
            (@setting PropagateGlobalValuesDown)
            (@setting AllowExternalSubcommands)
            (@arg profiler: --profiler [LIB] +global "Path to `libclang_rt.profile_*.a`")
            (@arg target: --target [TRIPLE] +global "Target triple which the covered program will run in")
            (@arg ("manifest-path"): --("manifest-path") [PATH] +global "Path to the manifest of the package")
            (@subcommand clean =>
                (about: "Clean coverage artifacts")
                (@setting UnifiedHelpMessage)
                (@arg gcda_only: --("gcda-only") "Remove the profile data only (*.gcda)")
                (@arg report: --report "Remove the coverage report too")
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
            )
        )
    ).get_matches()
}

/// Parses the command line arguments and forwards to [`report::generate`].
///
/// [`report::generate`]: report/fn.generate.html
fn generate_reports(cargo: &Cargo, matches: &ArgMatches) -> Result<()> {
    let allowed_source_types = matches.values_of("include").map_or(SOURCE_TYPE_DEFAULT, |it| SourceType::from_multi_str(it).expect("SourceType"));

    let template = matches.value_of_os("template").unwrap_or_else(|| OsStr::new("html"));
    let open_path = report::generate(cargo.cov_build_path(), template, allowed_source_types)?;

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
