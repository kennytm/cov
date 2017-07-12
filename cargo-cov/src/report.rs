//! Coverage report generation.
//!
//! # Template directory structure
//!
//! The coverage report produce two classes of files:
//!
//! * One summary page
//! * Source file pages, one page per source.
//!
//! `cargo cov` uses [Tera templates](https://github.com/Keats/tera#readme). Template files are stored using this
//! directory structure:
//!
//! ```text
//! cargo-cov/res/templates/«name»/
//!     config.toml
//!     tera/
//!         summary_template.ext
//!         file_template.ext
//!         ...
//!     static/
//!         common.css
//!         common.js
//!         ...
//! ```
//!
//! When rendered, the output will have this structure:
//!
//! ```text
//! /path/to/workspace/target/cov/report/
//!     static/
//!         common.css
//!         common.js
//!         ...
//!     summary.ext
//!     file_123.ext
//!     file_124.ext
//!     ...
//! ```
//!
//! # Summary page
//!
//! If a summary page is needed, add the following section to `config.toml`:
//!
//! ```toml
//! [summary]
//! template = "summary_template.ext"
//! output = "summary.ext"
//! ```
//!
//! The summary page will be rendered to the file `summary.ext` using this data:
//!
//! ```json
//! {
//!     "crate_path": "/path/to/workspace",
//!     "files": [
//!         {
//!             "symbol": 123,
//!             "path": "/path/to/workspace/src/lib.rs",
//!             "summary": {
//!                 "lines_count": 500,
//!                 "lines_covered": 499,
//!                 "branches_count": 700,
//!                 "branches_executed": 650,
//!                 "branches_taken": 520,
//!                 "functions_count": 40,
//!                 "functions_called": 39
//!             }
//!         },
//!         ...
//!     ]
//! }
//! ```
//!
//! # File pages
//!
//! If the file pages are needed, add the following section to `config.toml`:
//!
//! ```toml
//! [summary]
//! template = "file_template.ext"
//! output = "file_{{ symbol }}.ext"
//! ```
//!
//! The output filename itself is a Tera template. The file pages will be rendered using this data:
//!
//! ```json
//! {
//!     "crate_path": "/path/to/workspace",
//!     "symbol": 123,
//!     "path": "/path/to/workspace/src/lib.rs",
//!     "summary": {
//!         "lines_count": 500,
//!         ...
//!     },
//!     "lines": [
//!         {
//!             "line": 1,
//!             "source": "/// First line of the source code",
//!             "count": null,
//!             "branches": []
//!         },
//!         {
//!             "line": 2,
//!             "source": "pub fn second_line_of_source_code() {",
//!             "count": 12,
//!             "branches": [
//!                 {
//!                     "count": 6,
//!                     "symbol": 456,
//!                     "path": "/path/to/workspace/src/lib.rs",
//!                     "line": 3,
//!                     "column: 0
//!                 },
//!                 ...
//!             ]
//!         },
//!         ...
//!     ],
//!     "functions": [
//!         {
//!             "symbol": 789,
//!             "name": "_ZN10crate_name26second_line_of_source_code17hce04ea776f1a67beE",
//!             "line": 2,
//!             "column": 0,
//!             "summary": {
//!                 "blocks_count": 100,
//!                 "blocks_executed": 90,
//!                 "entry_count": 12,
//!                 "exit_count": 10,
//!                 "branches_count": 250,
//!                 "branches_executed": 225,
//!                 "branches_taken": 219
//!             }
//!         },
//!         ...
//!     ]
//! }
//! ```

use argparse::ReportConfig;
use error::{Result, ResultExt};
use sourcepath::{SourceType, identify_source_path};
use template::new as new_template;
use utils::{clean_dir, parent_3};

use copy_dir::copy_dir;
use cov::{self, Gcov, Graph, Interner, Report, Symbol};
use serde_json::Value;
use tera::{Context, Tera};

use std::ffi::OsStr;
use std::fs::{File, create_dir_all, read_dir};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

/// Entry point of `cargo cov report` subcommand. Renders the coverage report using a template.
pub fn generate(config: &ReportConfig) -> Result<Option<PathBuf>> {
    let report_path = &config.output_path;
    clean_dir(report_path).chain_err(|| "Cannot clean report directory")?;
    create_dir_all(report_path)?;

    let mut interner = Interner::new();
    let graph = create_graph(config, &mut interner).chain_err(|| "Cannot create graph")?;
    let report = graph.report();

    render(report_path, config.template_name, config.allowed_source_types, &report, &interner).chain_err(|| "Cannot render report")
}

/// Creates an analyzed [`Graph`] from all GCNO and GCDA inside the `target/cov/build` folder.
///
/// [`Graph`]: ../../cov/graph/struct.Graph.html
fn create_graph(config: &ReportConfig, interner: &mut Interner) -> cov::Result<Graph> {
    let mut graph = Graph::default();

    for &(extension, dir_path) in &[("gcno", &config.gcno_path), ("gcda", &config.gcda_path)] {
        progress!("Parsing", "{}/*.{}", dir_path.display(), extension);
        for entry in read_dir(dir_path)? {
            let path = entry?.path();
            if path.extension() == Some(OsStr::new(extension)) {
                trace!("merging {} {:?}", extension, path);
                graph.merge(Gcov::open(path, interner)?)?;
            }
        }
    }

    graph.analyze();
    Ok(graph)
}

/// Renders the `report` into `report_path` using a template.
///
/// If the template has a summary page, returns the path of the rendered summary.
fn render(report_path: &Path, template_name: &OsStr, allowed_source_types: SourceType, report: &Report, interner: &Interner) -> Result<Option<PathBuf>> {
    use toml::de::from_slice;

    let mut template_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    template_path.push("res");
    template_path.push("templates");
    template_path.push(template_name);
    trace!("using templates at {:?}", template_path);

    // Read the template configuration.
    template_path.push("config.toml");
    let mut config_file = File::open(&template_path).chain_err(|| format!("Cannot open template at `{}`", template_path.display()))?;
    let mut config_bytes = Vec::new();
    config_file.read_to_end(&mut config_bytes)?;
    let config: Config = from_slice(&config_bytes).chain_err(|| "Cannot read template configuration")?;

    // Copy the static resources if exist.
    template_path.set_file_name("static");
    if template_path.is_dir() {
        copy_dir(&template_path, report_path.join("static"))?;
    }

    template_path.set_file_name("tera");
    template_path.push("*");

    // The report path is at $crate/target/cov/report, so we call .parent() three times.
    let crate_path = parent_3(report_path).to_string_lossy();

    let mut tera = new_template(template_path.to_str().expect("UTF-8 template path"))?;

    let mut report_files = report
        .files
        .iter()
        .filter_map(|(&symbol, file)| {
            let path = &interner[symbol];
            let source_type = identify_source_path(path, &crate_path).0;
            if allowed_source_types.contains(source_type) {
                Some(ReportFileEntry {
                    symbol,
                    source_type,
                    path,
                    file,
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    report_files.sort_by_key(|entry| (entry.source_type, entry.path));

    let summary_path = if let Some(summary) = config.summary {
        Some(write_summary(report_path, &report_files, &tera, &crate_path, &summary).chain_err(|| "Cannot write summary")?)
    } else {
        None
    };

    if let Some(files_config) = config.files {
        tera.add_raw_template("<filename>", files_config.output)?;
        for entry in &report_files {
            write_file(report_path, interner, entry, &tera, &crate_path, files_config.template).chain_err(|| format!("Cannot write file at `{}`", entry.path))?;
        }
    }

    Ok(summary_path)
}

struct ReportFileEntry<'a> {
    symbol: Symbol,
    source_type: SourceType,
    path: &'a str,
    file: &'a ::cov::report::File,
}

#[derive(Deserialize, Debug)]
struct Config<'a> {
    #[serde(borrow)]
    summary: Option<FileConfig<'a>>,
    #[serde(borrow)]
    files: Option<FileConfig<'a>>,
}
#[derive(Deserialize, Debug)]
struct FileConfig<'a> {
    #[serde(borrow)]
    output: &'a str,
    #[serde(borrow)]
    template: &'a str,
}

/// Renders the summary page.
fn write_summary(report_path: &Path, report_files: &[ReportFileEntry], tera: &Tera, crate_path: &str, config: &FileConfig) -> Result<PathBuf> {
    let path = report_path.join(config.output);
    let mut context = Context::new();

    let files = report_files
        .iter()
        .map(|entry| {
            json!({
                "symbol": entry.symbol,
                "path": entry.path,
                "summary": entry.file.summary(),
            })
        })
        .collect::<Vec<_>>();

    context.add("crate_path", &crate_path);
    context.add("files", &files);
    let rendered = tera.render(config.template, &context)?;
    let mut summary_file = File::create(&path)?;
    summary_file.write_all(rendered.as_bytes())?;
    progress!("Created", "{}", path.display());
    Ok(path)
}

/// Renders report for a source path.
fn write_file(report_path: &Path, interner: &Interner, entry: &ReportFileEntry, tera: &Tera, crate_path: &str, template_name: &str) -> Result<()> {
    let mut context = Context::new();

    let mut lines = Vec::new();
    let mut source_line_number = 1;

    // Read the source file.
    if let Ok(source_file) = File::open(entry.path) {
        let source_file = BufReader::new(source_file);
        for source_line in source_file.lines() {
            let (count, branches) = if let Some(line) = entry.file.lines.get(&source_line_number) {
                let (count, branches) = serialize_line(line, interner);
                (Some(count), branches)
            } else {
                (None, Vec::new())
            };
            lines.push(json!({
                "line": source_line_number,
                "source": source_line?,
                "count": count,
                "branches": branches,
            }));
            source_line_number += 1;
        }
    }

    // Add the remaining lines absent from the source file.
    lines.extend(entry.file.lines.range(source_line_number..).map(|(line_number, line)| {
        let (count, branches) = serialize_line(line, interner);
        json!({
            "line": *line_number,
            "count": Some(count),
            "source": Value::Null,
            "branches": branches,
        })
    }));

    // Collect function info
    let functions = entry
        .file
        .functions
        .iter()
        .map(|f| {
            let name = &interner[f.name];
            json!({
                "symbol": f.name,
                "name": name,
                "line": f.line,
                "column": f.column,
                "summary": &f.summary,
            })
        })
        .collect::<Vec<_>>();

    context.add("crate_path", &crate_path);
    context.add("symbol", &entry.symbol);
    context.add("path", &entry.path);
    context.add("summary", &entry.file.summary());
    context.add("lines", &lines);
    context.add("functions", &functions);

    let filename = tera.render("<filename>", &context)?;
    let path = report_path.join(filename);
    let rendered = tera.render(template_name, &context)?;
    let mut file_file = File::create(path)?;
    file_file.write_all(rendered.as_bytes())?;

    Ok(())
}

/// Serializes a source line as a branch target into JSON value.
fn serialize_line(line: &::cov::report::Line, interner: &Interner) -> (u64, Vec<Value>) {
    (
        line.count,
        line.branches
            .iter()
            .map(|branch| {
                json!({
                    "count": branch.count,
                    "symbol": branch.filename,
                    "path": &interner[branch.filename],
                    "line": branch.line,
                    "column": branch.column,
                })
            })
            .collect(),
    )
}
