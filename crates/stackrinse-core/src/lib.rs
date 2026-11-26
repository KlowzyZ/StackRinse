//! Read-only artifact inventory. Plans are snapshots, never deletion capabilities.
mod policy;
mod rules;
mod scanner;
use serde::{ser::SerializeStruct, Serialize, Serializer};
use std::path::PathBuf;

pub use scanner::{scan, ScanError};

#[derive(Default, Debug, Clone)]
pub struct ScanOptions {
    /// Absolute path prefixes or directory basenames.
    pub exclusions: Vec<PathBuf>,
    pub protected_projects: Vec<PathBuf>,
}

/// Effective inputs after the scanner has resolved paths and collapsed nested roots.
#[derive(Default, Debug, Clone, Serialize)]
pub struct ScanScope {
    pub roots: Vec<PathBuf>,
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
    #[serde(skip_serializing)]
    pub ecosystem: &'static str,
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

#[derive(Default, Clone, Debug)]
pub struct Report {
    pub scope: ScanScope,
    pub projects: Vec<Project>,
    pub warnings: Vec<String>,
    pub cancelled: bool,
    pub directories: u64,
}

#[derive(Default, Clone, Debug, serde::Deserialize, Serialize)]
pub struct InventoryTotals {
    pub logical_bytes: u64,
    pub reviewable_bytes: u64,
    pub protected_bytes: u64,
}

fn protected(project: &Project, candidate: &Candidate) -> bool {
    project.protected
        || !candidate.size_complete
        || candidate.classification == Classification::Protected
}

impl Report {
    pub fn inventory_complete(&self) -> bool {
        !self.cancelled
            && self.warnings.is_empty()
            && self.projects.iter().all(|project| {
                project
                    .candidates
                    .iter()
                    .all(|candidate| candidate.size_complete)
            })
    }

    /// Computed from the current view, including after CLI size filtering.
    pub fn totals(&self) -> InventoryTotals {
        let mut totals = InventoryTotals::default();
        for project in &self.projects {
            for candidate in &project.candidates {
                let bytes = candidate.logical_bytes;
                totals.logical_bytes = totals.logical_bytes.saturating_add(bytes);
                if protected(project, candidate) {
                    totals.protected_bytes = totals.protected_bytes.saturating_add(bytes);
                } else if !self.cancelled {
                    totals.reviewable_bytes = totals.reviewable_bytes.saturating_add(bytes);
                }
            }
        }
        totals
    }
}

impl Serialize for Report {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut view = serializer.serialize_struct("Report", 5)?;
        view.serialize_field("projects", &self.projects)?;
        view.serialize_field("warnings", &self.warnings)?;
        view.serialize_field("cancelled", &self.cancelled)?;
        view.serialize_field("directories", &self.directories)?;
        view.serialize_field("totals", &self.totals())?;
        view.end()
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Plan {
    candidates: Vec<Candidate>,
    logical_bytes: u64,
}

impl Plan {
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }
    pub fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
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
        .filter(|_| !report.cancelled)
        .flat_map(|p| {
            p.candidates
                .iter()
                .filter(move |c| !protected(p, c) && selected(p, c))
        })
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
