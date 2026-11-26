use super::Snapshot;
use crate::settings::Result;
use serde::{Deserialize, Serialize};
use std::{
    io::{self, BufWriter, Write},
    path::Path,
};
use tempfile::NamedTempFile;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Json,
    Csv,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Source {
    Fresh,
    History { id: i64 },
}

pub fn save(path: &Path, format: Format, source: Source, snapshot: &Snapshot) -> Result<()> {
    if !path.is_absolute()
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case(format.extension()))
    {
        return Err(format!(
            "Choose an absolute destination ending in .{}.",
            format.extension()
        ));
    }
    let parent = path.parent().ok_or("Choose a destination folder.")?;
    let mut temp = NamedTempFile::new_in(parent).map_err(export_error)?;
    {
        let mut output = BufWriter::new(temp.as_file_mut());
        match format {
            Format::Json => write_json(&mut output, source, snapshot)?,
            Format::Csv => write_csv(&mut output, source, snapshot).map_err(export_error)?,
        }
        output.flush().map_err(export_error)?;
    }
    temp.as_file().sync_all().map_err(export_error)?;
    temp.persist_noclobber(path).map_err(|error| {
        if error.error.kind() == io::ErrorKind::AlreadyExists {
            "A file already exists there. Choose a new name for the report.".into()
        } else {
            export_error(error.error)
        }
    })?;
    Ok(())
}

fn export_error(error: io::Error) -> String {
    format!("Could not export the inventory: {error}")
}

fn write_json(output: impl Write, source: Source, snapshot: &Snapshot) -> Result<()> {
    #[derive(Serialize)]
    struct Report<'a> {
        format_version: u32,
        kind: &'static str,
        source: Source,
        summary: Summary<'a>,
        inventory: &'a super::Inventory,
    }
    // A report contains observations and root paths. The internal scope
    // fingerprint and retention state have no role in the exported format.
    #[derive(Serialize)]
    struct Summary<'a> {
        completed_at_ms: i64,
        discovery_rule_version: u32,
        duration_ms: u64,
        complete: bool,
        project_count: usize,
        candidate_count: usize,
        incomplete_candidate_count: usize,
        warning_count: usize,
        directories: u64,
        totals: &'a stackrinse_core::InventoryTotals,
    }
    let summary = &snapshot.summary;
    serde_json::to_writer_pretty(
        output,
        &Report {
            format_version: 1,
            kind: "logical_artifact_inventory",
            source,
            summary: Summary {
                completed_at_ms: summary.completed_at_ms,
                discovery_rule_version: summary.discovery_rule_version,
                duration_ms: summary.duration_ms,
                complete: summary.complete,
                project_count: summary.project_count,
                candidate_count: summary.candidate_count,
                incomplete_candidate_count: summary.incomplete_candidate_count,
                warning_count: summary.warning_count,
                directories: summary.directories,
                totals: &summary.totals,
            },
            inventory: &snapshot.inventory,
        },
    )
    .map_err(|error| format!("Could not serialize the inventory: {error}"))
}

fn csv_row(output: &mut impl Write, cells: [&str; 16]) -> io::Result<()> {
    for (index, value) in cells.into_iter().enumerate() {
        if index != 0 {
            output.write_all(b",")?;
        }
        let effective = value.trim_start_matches(|c: char| {
            c.is_whitespace() || c.is_control() || matches!(c, '\u{feff}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{2069}')
        });
        let spreadsheet_prefix = value.starts_with(['\t', '\r', '\n'])
            || effective.starts_with(['=', '+', '-', '@', '＝', '＋', '－', '＠']);
        output.write_all(b"\"")?;
        if spreadsheet_prefix {
            output.write_all(b"'")?;
        }
        for (piece_index, piece) in value.split('"').enumerate() {
            if piece_index > 0 {
                output.write_all(b"\"\"")?;
            }
            output.write_all(piece.as_bytes())?;
        }
        output.write_all(b"\"")?;
    }
    output.write_all(b"\r\n")
}

#[derive(Default)]
struct Row<'a> {
    record: &'a str,
    completed_at_ms: Option<i64>,
    duration_ms: Option<u64>,
    completeness: &'a str,
    project_count: Option<usize>,
    candidate_count: Option<usize>,
    logical_bytes: Option<u64>,
    reviewable_bytes: Option<u64>,
    protected_bytes: Option<u64>,
    path: &'a str,
    ecosystem: &'a str,
    category: &'a str,
    message: &'a str,
    source: &'a str,
    project_details: &'a str,
    discovery_rule_version: Option<u32>,
}

impl Row<'_> {
    fn write(self, output: &mut impl Write) -> io::Result<()> {
        fn number(value: Option<impl ToString>) -> String {
            value.map(|value| value.to_string()).unwrap_or_default()
        }
        csv_row(
            output,
            [
                self.record,
                &number(self.completed_at_ms),
                &number(self.duration_ms),
                self.completeness,
                &number(self.project_count),
                &number(self.candidate_count),
                &number(self.logical_bytes),
                &number(self.reviewable_bytes),
                &number(self.protected_bytes),
                self.path,
                self.ecosystem,
                self.category,
                self.message,
                self.source,
                self.project_details,
                &number(self.discovery_rule_version),
            ],
        )
    }
}

fn write_csv(output: &mut impl Write, source: Source, snapshot: &Snapshot) -> io::Result<()> {
    // The UTF-8 marker helps spreadsheet programs recognize Unicode on open.
    output.write_all(b"\xef\xbb\xbf")?;
    csv_row(
        output,
        [
            "record",
            "completed_at_unix_ms",
            "duration_ms",
            "completeness",
            "project_count",
            "candidate_count",
            "logical_bytes",
            "reviewable_bytes",
            "protected_bytes",
            "path",
            "ecosystem",
            "category",
            "message",
            "source",
            "project_details",
            "discovery_rule_version",
        ],
    )?;
    let summary = &snapshot.summary;
    let inventory = &snapshot.inventory;
    Row {
        record: "scan",
        completed_at_ms: Some(summary.completed_at_ms),
        discovery_rule_version: Some(summary.discovery_rule_version),
        duration_ms: Some(summary.duration_ms),
        completeness: if summary.complete {
            "complete"
        } else {
            "incomplete"
        },
        project_count: Some(summary.project_count),
        candidate_count: Some(summary.candidate_count),
        logical_bytes: Some(summary.totals.logical_bytes),
        reviewable_bytes: Some(summary.totals.reviewable_bytes),
        protected_bytes: Some(summary.totals.protected_bytes),
        source: match source {
            Source::Fresh => "fresh_scan",
            Source::History { .. } => "saved_history",
        },
        project_details: if inventory.projects.is_some() {
            "included"
        } else {
            "unavailable"
        },
        ..Row::default()
    }
    .write(output)?;
    for root in &inventory.roots {
        Row {
            record: "root",
            path: &root.to_string_lossy(),
            ..Row::default()
        }
        .write(output)?;
    }
    for exclusion in &inventory.exclusions {
        Row {
            record: "exclusion",
            message: &exclusion.to_string_lossy(),
            ..Row::default()
        }
        .write(output)?;
    }
    for warning in &inventory.warnings {
        Row {
            record: "warning",
            message: warning,
            ..Row::default()
        }
        .write(output)?;
    }
    for (name, bytes) in &inventory.ecosystems {
        Row {
            record: "ecosystem",
            ecosystem: name,
            logical_bytes: Some(*bytes),
            ..Row::default()
        }
        .write(output)?;
    }
    for (name, bytes) in &inventory.categories {
        Row {
            record: "category",
            category: name,
            logical_bytes: Some(*bytes),
            ..Row::default()
        }
        .write(output)?;
    }
    if let Some(projects) = &inventory.projects {
        for project in projects {
            Row {
                record: "project",
                candidate_count: Some(project.candidate_count),
                logical_bytes: Some(project.logical_bytes),
                path: &project.path.to_string_lossy(),
                ecosystem: &project.ecosystems.join(" | "),
                ..Row::default()
            }
            .write(output)?;
        }
    }
    Ok(())
}
