//! Read-only artifact inventory. Plans are snapshots, never deletion capabilities.
mod policy;
mod rules;
mod scanner;
use serde::Serialize;
use std::path::PathBuf;

pub use scanner::{scan, ScanError};

#[derive(Default, Debug, Clone)]
pub struct ScanOptions {
    /// Absolute path prefixes or directory basenames.
    pub exclusions: Vec<PathBuf>,
    pub protected_projects: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Progress {
    pub directories: u64,
    pub projects: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    Rebuildable,
    ReviewRequired,
    Protected,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cost {
    Low,
    Medium,
    High,
    Unknown,
}

#[derive(Clone, Debug, Serialize)]
pub struct Candidate {
    pub path: PathBuf,
    pub category: String,
    pub logical_bytes: u64,
    /// False when policy boundaries or read errors skipped entries.
    pub size_complete: bool,
    pub classification: Classification,
    pub reason: String,
    pub rebuild: String,
    pub cost: Cost,
}

#[derive(Clone, Debug, Serialize)]
pub struct Project {
    pub path: PathBuf,
    pub name: String,
    pub ecosystems: Vec<String>,
    /// Includes protected inventory. This is not a free-space measurement.
    pub logical_bytes: u64,
    /// Most recent recognized manifest mtime, in Unix seconds.
    pub last_activity: Option<u64>,
    pub protected: bool,
    /// Mandatory project protection, independent of user preferences.
    pub boundary_reason: Option<String>,
    pub candidates: Vec<Candidate>,
}

#[derive(Default, Clone, Debug, Serialize)]
pub struct Report {
    pub projects: Vec<Project>,
    pub warnings: Vec<String>,
    pub cancelled: bool,
    pub directories: u64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    pub candidates: Vec<Candidate>,
    pub logical_bytes: u64,
}

/// Inventory eligible for further review. No executable operation is issued.
pub fn dry_run(report: &Report) -> Plan {
    dry_run_selected(report, |_, _| true)
}

pub fn dry_run_selected(report: &Report, selected: impl Fn(&Project, &Candidate) -> bool) -> Plan {
    let selected = &selected;
    let candidates: Vec<_> = report
        .projects
        .iter()
        .filter(|p| !report.cancelled && !p.protected)
        .flat_map(|p| p.candidates.iter().filter(move |c| selected(p, c)))
        .filter(|c| c.classification != Classification::Protected)
        .cloned()
        .collect();
    let logical_bytes = candidates
        .iter()
        .fold(0u64, |n, c| n.saturating_add(c.logical_bytes));
    Plan {
        candidates,
        logical_bytes,
    }
}
