extern crate cov;
extern crate diff;
extern crate serde;
extern crate serde_json;
extern crate termcolor;

use cov::*;
use serde::Serialize;
use serde_json::ser::{PrettyFormatter, Serializer};
use termcolor::{Color, ColorChoice, ColorSpec, StandardStream, WriteColor};

use std::ffi::OsStr;
use std::fs::{File, read_dir};
use std::io::{self, Read, Write};
use std::path::Path;
use std::process::exit;

fn main() {
    run().expect("IO");
}

fn run() -> io::Result<()> {
    let allowed_extensions = [OsStr::new("gcc7"), OsStr::new("clang"), OsStr::new("rustc")];
    let mut failed_tests = 0;

    let stdout = StandardStream::stdout(ColorChoice::Auto);
    let mut lock = stdout.lock();

    for entry in read_dir("test-data")? {
        let entry = entry?;
        let path = entry.path();
        if let Some(extension) = path.extension() {
            if allowed_extensions.contains(&extension) && entry.file_type()?.is_dir() {
                write!(lock, "test {} ... ", path.display())?;
                lock.flush()?;
                if !print_test_result(&mut lock, test(&path))? {
                    failed_tests += 1;
                }
            }
        }
    }

    if failed_tests != 0 {
        writeln!(lock, "\ntest result: {} failed.\n", failed_tests)?;
        exit(101);
    } else {
        writeln!(lock, "\ntest result: ok.\n")?;
    }

    Ok(())
}

fn test(path: &Path) -> Result<(String, String)> {
    let mut interner = Interner::new();
    let mut graph = Graph::new();

    let gcno_path = path.join("x.gcno");
    graph.merge(Gcov::open(&gcno_path, &mut interner)?)?;

    let mut gcda_path = gcno_path;
    gcda_path.set_extension("gcda");
    graph.merge(Gcov::open(&gcda_path, &mut interner)?)?;

    graph.analyze();
    let report = graph.report();
    let mut serializer = Serializer::with_formatter(Vec::new(), PrettyFormatter::with_indent(b"    "));
    report.with_interner(&interner).serialize(&mut serializer)?;
    let actual_report = String::from_utf8(serializer.into_inner()).expect("UTF-8 JSON");

    let mut report_path = gcda_path;
    report_path.set_extension("json");
    let mut expected_report_file = File::open(report_path)?;
    let mut expected_report = String::new();
    expected_report_file.read_to_string(&mut expected_report)?;

    Ok((actual_report, expected_report))
}

fn print_test_result<W: Write + WriteColor>(mut lock: W, result: Result<(String, String)>) -> io::Result<bool> {
    Ok(match result {
        Ok((actual_report, expected_report)) => {
            let success = actual_report == expected_report;
            if success {
                lock.set_color(ColorSpec::new().set_fg(Some(Color::Green)))?;
                writeln!(lock, "ok")?;
            } else {
                lock.set_color(ColorSpec::new().set_fg(Some(Color::Red)))?;
                writeln!(lock, "FAILED")?;
                for d in diff::lines(&actual_report, &expected_report) {
                    let (color, prefix, line) = match d {
                        diff::Result::Left(line) => (Color::Green, '+', line),
                        diff::Result::Both(line, _) => (Color::White, ' ', line),
                        diff::Result::Right(line) => (Color::Red, '-', line),
                    };
                    lock.set_color(ColorSpec::new().set_fg(Some(color)))?;
                    writeln!(lock, "{} {}", prefix, line)?;
                }
                writeln!(lock)?;
            }
            lock.reset()?;
            success
        },
        Err(e) => {
            lock.set_color(ColorSpec::new().set_fg(Some(Color::Magenta)))?;
            writeln!(lock, "ERRORED")?;
            lock.set_color(ColorSpec::new().set_fg(Some(Color::Red)).set_intense(true).set_bold(true))?;
            write!(lock, "error: ")?;
            lock.reset()?;
            writeln!(lock, "{}\n", e)?;
            false
        },
    })
}
