use error::{Result, ResultExt};
use sourcepath::{SourceType, identify_source_path};
use template::new as new_template;
use utils::{clean_dir, parent_3};

use copy_dir::copy_dir;
use cov::{Gcov, Graph, Interner, Report, Symbol};
use serde_json::Value;
use tera::{Context, Tera};

use std::ffi::OsStr;
use std::fs::{File, create_dir_all, read_dir};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};

pub fn generate(cov_build_path: &Path, template_name: &OsStr, allowed_source_types: SourceType) -> Result<Option<PathBuf>> {
    let report_path = cov_build_path.with_file_name("report");
    clean_dir(&report_path)?;
    create_dir_all(&report_path)?;

    let mut interner = Interner::new();
    let graph = create_graph(cov_build_path, &mut interner)?;
    let report = graph.report();

    render(&report_path, template_name, allowed_source_types, &report, &interner)
}

fn create_graph(cov_build_path: &Path, interner: &mut Interner) -> Result<Graph> {
    let mut graph = Graph::default();

    for extension in &["gcno", "gcda"] {
        for entry in read_dir(cov_build_path.join(extension))? {
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


fn render(report_path: &Path, template_name: &OsStr, allowed_source_types: SourceType, report: &Report, interner: &Interner) -> Result<Option<PathBuf>> {
    use toml::de::from_slice;

    let mut template_path = PathBuf::from(file!());
    template_path.pop();
    template_path.set_file_name("res");
    template_path.push("templates");
    template_path.push(template_name);
    trace!("using templates at {:?}", template_path);

    // Read the template configuration.
    template_path.push("config.toml");
    let mut config_file = File::open(&template_path)?;
    let mut config_bytes = Vec::new();
    config_file.read_to_end(&mut config_bytes)?;
    let config: Config = from_slice(&config_bytes)?;

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
        Some(write_summary(report_path, &report_files, &tera, &crate_path, &summary)?)
    } else {
        None
    };

    if let Some(files_config) = config.files {
        tera.add_raw_template("<filename>", files_config.output)?;
        for entry in &report_files {
            write_file(report_path, interner, entry, &tera, &crate_path, files_config.template)?;
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
