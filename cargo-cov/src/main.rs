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
extern crate serde;
extern crate shell_escape;
extern crate tempdir;
extern crate tera;
extern crate termcolor;
extern crate toml;
extern crate walkdir;

/// Prints a progress, similar to the cargo output.
macro_rules! progress {
    ($tag:expr, $fmt:expr $(, $args:expr)*) => {{
        (|| -> ::std::io::Result<()> {
            use ::termcolor::*;
            use ::std::io::Write;
            let stream = StandardStream::stderr(ColorChoice::Auto);
            let mut lock = stream.lock();
            lock.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
            write!(lock, "{:>12} ", $tag)?;
            lock.reset()?;
            writeln!(lock, $fmt $(, $args)*)?;
            Ok(())
        })().expect("print progress")
    }}
}

/// Prints a warning, similar to cargo output.
macro_rules! warning {
    ($fmt:expr $(, $args:expr)*) => {{
        (|| -> ::std::io::Result<()> {
            use ::termcolor::*;
            use ::std::io::Write;
            let stream = StandardStream::stderr(ColorChoice::Auto);
            let mut lock = stream.lock();
            lock.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)).set_bold(true))?;
            write!(lock, "warning: ")?;
            lock.reset()?;
            writeln!(lock, $fmt $(, $args)*)?;
            Ok(())
        })().expect("print warning")
    }}
}

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
use clap::{ArgMatches, OsValues};
use either::Either;
use error::{Error, Result};
use sourcepath::*;
use termcolor::*;

use std::ffi::OsStr;
use std::io::{self, Write};
use std::iter::empty;
use std::process::exit;

fn main() {
    if let Err(error) = run() {
        print_error(error).expect("error while printing error 🤷");
        exit(1);
    }
}

fn print_error(error: Error) -> io::Result<()> {
    let stream = StandardStream::stderr(ColorChoice::Auto);
    let mut lock = stream.lock();

    for (i, e) in error.iter().enumerate() {
        if i == 0 {
            lock.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_intense(true).set_bold(true))?;
            write!(lock, "error: ")?;
        } else {
            lock.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true))?;
            write!(lock, "caused by: ")?;
        }
        lock.reset()?;
        writeln!(lock, "{}", e)?;
    }
    if let Some(backtrace) = error.backtrace() {
        writeln!(lock, "\n{:?}", backtrace)?;
    }
    Ok(())
}

fn run() -> Result<()> {
    let matches = parse_args();
    env_logger::init().unwrap();

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

    let mut special_args = SpecialMap::with_capacity(3);
    update_from_clap(matches, &mut special_args);

    let (subcommand, matches) = matches.subcommand();
    let matches = matches.expect("matches");
    update_from_clap(matches, &mut special_args);

    let forward_args = match matches.values_of_os("") {
        Some(args) => normalize(args, &mut special_args),
        None => Vec::new(),
    };
    let cargo = Cargo::new(special_args, forward_args)?;

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
            print_unknown_subcommand(subcommand)?;
        },
    }

    Ok(())
}


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

fn parse_args() -> clap::ArgMatches<'static> {
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


fn generate_reports(cargo: &Cargo, matches: &ArgMatches) -> Result<()> {
    let allowed_source_types = matches.values_of("include").map_or(SOURCE_TYPE_DEFAULT, |it| SourceType::from_multi_str(it).unwrap());

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


fn print_unknown_subcommand(subcommand: &str) -> io::Result<()> {
    let stream = StandardStream::stderr(ColorChoice::Auto);
    let mut lock = stream.lock();

    lock.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_bold(true))?;
    write!(lock, "error: ")?;
    lock.reset()?;
    write!(lock, "unrecognized command `")?;
    lock.set_color(ColorSpec::new().set_bold(true))?;
    write!(lock, "cargo cov ")?;
    lock.set_color(ColorSpec::new().set_fg(Some(Color::Yellow)).set_bold(true))?;
    write!(lock, "{}", subcommand)?;
    lock.reset()?;
    write!(lock, "`.\n\nTry `")?;
    lock.set_color(ColorSpec::new().set_bold(true))?;
    write!(lock, "cargo cov ")?;
    lock.set_color(ColorSpec::new().set_fg(Some(Color::Green)).set_bold(true))?;
    write!(lock, "--help")?;
    lock.reset()?;
    writeln!(lock, "` for a list of valid commands.")?;
    Ok(())
}
