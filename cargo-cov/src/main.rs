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
extern crate colored;
extern crate copy_dir;
extern crate env_logger;
extern crate glob;
extern crate md5;
extern crate natord;
extern crate open;
extern crate rustc_demangle;
extern crate serde;
extern crate tera;
extern crate toml;

macro_rules! progress {
    ($tag:expr, $fmt:expr $(, $args:expr)*) => {{
        use colored::Colorize;
        writeln!(::std::io::stderr(), concat!("{:>12} ", $fmt), $tag.green().bold() $(, $args)*).unwrap();
    }}
}

macro_rules! warning {
    ($fmt:expr $(, $args:expr)*) => {{
        use colored::Colorize;
        writeln!(::std::io::stderr(), concat!("{} ", $fmt), "warning:".yellow().bold() $(, $args)*).unwrap();
    }}
}

mod error;
mod utils;
mod lookup;
mod argparse;
mod cargo;
mod report;
mod template;
mod sourcepath;

use argparse::*;
use cargo::Cargo;
use clap::ArgMatches;
use error::Result;
use sourcepath::*;
use std::ffi::OsStr;
use std::io::Write;

#[cfg(not(debug_assertions))]
fn main() {
    if let Err(error) = run() {
        writeln!(stderr(), "{} {}", "error:".red().bold(), error).unwrap();
    }
}

#[cfg(debug_assertions)]
quick_main!(run);

fn run() -> Result<()> {
    let matches = parse_args();
    env_logger::init().unwrap();

    let matches = matches.subcommand_matches("cov").expect("This command should be executed as `cargo cov`.");
    debug!("matches = {:?}", matches);

    let mut special_args = SpecialMap::with_capacity(3);
    update_from_clap(matches, &mut special_args);

    let (subcommand, matches) = matches.subcommand();
    let matches = matches.unwrap();
    update_from_clap(matches, &mut special_args);

    let forward_args = match matches.values_of_os("args") {
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
        _ => unreachable!("unknown subcommand {}", subcommand),
    }

    Ok(())
}


fn parse_args() -> clap::ArgMatches<'static> {
    clap_app!(@app (app_from_crate!())
        (@subcommand cov =>
            (bin_name: "cargo cov")
            (@setting DeriveDisplayOrder)
            (@setting SubcommandRequiredElseHelp)
            (@setting GlobalVersion)
            (@setting PropagateGlobalValuesDown)
            (@arg profiler: --profiler [LIB] +global "Path to `libclang_rt.profile_*.a`")
            (@arg target: --target [TRIPLE] +global "Target triple which the covered program will run in")
            (@arg ("manifest-path"): --("manifest-path") [PATH] +global "Path to the manifest of the package")
            (@subcommand build =>
                (about: "Compile the crate and produce coverage data (*.gcno)")
                (@setting TrailingValues) // FIXME: TrailingValues is undocumented and may be wrong.
                (@arg args: [ARGS]... "arguments to pass to `cargo build`")
            )
            (@subcommand test =>
                (about: "Test the crate and produce profile data (*.gcda)")
                (@setting TrailingValues)
                (@arg args: [ARGS]... "arguments to pass to `cargo test`")
            )
            (@subcommand run =>
                (about: "Run a program and produces profile data (*.gcda)")
                (@arg args: [ARGS]... "arguments to pass to `cargo run`")
            )
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
