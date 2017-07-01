//! Print colored text.
//!
//! Provides functions and macros that simulate the `cargo` output style.

use error::Error;

use termcolor::*;

use std::io::{Result, Write};

/// Prints a progress (green text), similar to the cargo output.
macro_rules! progress {
    ($tag:expr, $fmt:expr $(, $args:expr)*) => {{
        #[cfg_attr(feature="cargo-clippy", allow(redundant_closure_call))]
        // ^ False positive, see https://github.com/Manishearth/rust-clippy/issues/1684
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

/// Prints a warning (yellow text), similar to cargo output.
macro_rules! warning {
    ($fmt:expr $(, $args:expr)*) => {{
        #[cfg_attr(feature="cargo-clippy", allow(redundant_closure_call))]
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

/// Prints an error and the causes.
pub fn print_error(error: &Error) -> Result<()> {
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

/// Prints a help message that an unknown subcommand is used for `cargo cov`.
pub fn print_unknown_subcommand(subcommand: &str) -> Result<()> {
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
