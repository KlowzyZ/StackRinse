pub mod export;

use crate::settings::Result;
use rusqlite::{
    limits::Limit, params, Connection, OpenFlags, OptionalExtension, Transaction,
    TransactionBehavior,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use stackrinse_core::{InventoryTotals, Report, DISCOVERY_RULE_VERSION};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};

const SCHEMA_VERSION: i64 = 1;
const APPLICATION_ID: i64 = 0x5352_494e;
const DEFAULT_RETENTION: u32 = 50;
const MAX_RETENTION: u32 = 100;
const MAX_PAYLOAD_BYTES: usize = 24 * 1024 * 1024;
const MAX_INVENTORY_BYTES: usize = 4 * 1024 * 1024;
const MAX_DATABASE_BYTES: u64 = 32 * 1024 * 1024;
const MAX_PROJECTS: usize = 20_000;
const DISPLAY_LIMIT: usize = 6;

#[derive(Clone, Copy)]
pub struct ScanTiming {
    pub completed_at_ms: i64,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Summary {
    completed_at_ms: i64,
    duration_ms: u64,
    scope_key: String,
    #[serde(default = "legacy_rule_version")]
    discovery_rule_version: u32,
    root_hint: String,
    root_count: usize,
    project_count: usize,
    candidate_count: usize,
    warning_count: usize,
    incomplete_candidate_count: usize,
    directories: u64,
    complete: bool,
    totals: InventoryTotals,
}

fn legacy_rule_version() -> u32 {
    1 // Sprint 4 stored this version in the fingerprint only.
}

#[derive(Serialize)]
pub struct Entry {
    id: i64,
    #[serde(flatten)]
    summary: Summary,
}

#[derive(Serialize)]
pub struct HistoryIndex {
    retention: u32,
    entries: Vec<Entry>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Project {
    path: PathBuf,
    ecosystems: Vec<String>,
    logical_bytes: u64,
    candidate_count: usize,
}

type Breakdown = BTreeMap<String, u64>;

#[derive(Debug, Deserialize, Serialize)]
struct Inventory {
    roots: Vec<PathBuf>,
    exclusions: Vec<PathBuf>,
    warnings: Vec<String>,
    ecosystems: Breakdown,
    categories: Breakdown,
    projects: Option<Vec<Project>>,
}

pub struct Snapshot {
    summary: Summary,
    inventory: Inventory,
}

#[derive(Serialize)]
pub struct Detail {
    entry: Entry,
    roots: Vec<PathBuf>,
    exclusions: Vec<PathBuf>,
    warnings: Vec<String>,
    ecosystems: Breakdown,
    categories: Breakdown,
    project_details_available: bool,
    largest_projects: Vec<Project>,
}

#[derive(Serialize)]
struct BucketChange {
    name: String,
    before: u64,
    after: u64,
}

#[derive(Serialize)]
struct ProjectChange {
    path: PathBuf,
    before: Option<u64>,
    after: Option<u64>,
}

#[derive(Serialize)]
struct ProjectChanges {
    count: usize,
    projects: Vec<ProjectChange>,
}

#[derive(Serialize)]
pub struct Comparison {
    logical_bytes_delta: i128,
    ecosystems: Vec<BucketChange>,
    categories: Vec<BucketChange>,
    growth: Option<ProjectChanges>,
    discovered: Option<ProjectChanges>,
    no_longer_observed: Option<ProjectChanges>,
}

fn capped_text(value: &str, max: usize) -> String {
    if value.len() <= max {
        return value.to_owned();
    }
    let mut end = max;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}…", &value[..end])
}

fn add_bytes(breakdown: &mut Breakdown, name: &str, bytes: u64) {
    if let Some(total) = breakdown.get_mut(name) {
        *total = total.saturating_add(bytes);
    } else {
        breakdown.insert(name.to_owned(), bytes);
    }
}

impl Snapshot {
    pub fn capture(report: &Report, timing: ScanTiming) -> Result<Self> {
        if report.cancelled {
            return Err("Cancelled scans are excluded from history and export.".into());
        }
        let scope_json = serde_json::to_vec(&(DISCOVERY_RULE_VERSION, &report.scope))
            .map_err(|error| error.to_string())?;
        let mut ecosystems = Breakdown::new();
        let mut categories = Breakdown::new();
        let mut candidate_count = 0;
        let mut incomplete_candidate_count = 0;
        let mut projects = (report.projects.len() <= MAX_PROJECTS).then(Vec::new);
        let mut project_text_bytes = 0usize;
        for project in &report.projects {
            candidate_count += project.candidates.len();
            for candidate in &project.candidates {
                incomplete_candidate_count += usize::from(!candidate.size_complete);
                add_bytes(
                    &mut ecosystems,
                    candidate.ecosystem,
                    candidate.logical_bytes,
                );
                add_bytes(
                    &mut categories,
                    &candidate.category,
                    candidate.logical_bytes,
                );
            }
            if let Some(projects_list) = &mut projects {
                project_text_bytes = project_text_bytes.saturating_add(
                    project.path.as_os_str().len()
                        + project.ecosystems.iter().map(String::len).sum::<usize>(),
                );
                if project_text_bytes > MAX_INVENTORY_BYTES / 2 {
                    projects = None;
                } else {
                    projects_list.push(Project {
                        path: project.path.clone(),
                        ecosystems: project.ecosystems.clone(),
                        logical_bytes: project.logical_bytes,
                        candidate_count: project.candidates.len(),
                    });
                }
            }
        }

        Ok(Self {
            summary: Summary {
                completed_at_ms: timing.completed_at_ms,
                duration_ms: timing.duration_ms,
                scope_key: format!("{:x}", Sha256::digest(scope_json)),
                discovery_rule_version: DISCOVERY_RULE_VERSION,
                root_hint: report.scope.roots.first().map_or_else(String::new, |root| {
                    capped_text(&root.to_string_lossy(), 512)
                }),
                root_count: report.scope.roots.len(),
                project_count: report.projects.len(),
                candidate_count,
                warning_count: report.warnings.len(),
                incomplete_candidate_count,
                directories: report.directories,
                complete: report.inventory_complete(),
                totals: report.totals(),
            },
            inventory: Inventory {
                roots: report.scope.roots.clone(),
                exclusions: report.scope.exclusions.clone(),
                warnings: report
                    .warnings
                    .iter()
                    .map(|warning| capped_text(warning, 2048))
                    .collect(),
                ecosystems,
                categories,
                projects,
            },
        })
    }

    fn entry(&self, id: i64) -> Entry {
        Entry {
            id,
            summary: self.summary.clone(),
        }
    }

    fn detail(mut self, id: i64) -> Detail {
        let entry = self.entry(id);
        let project_details_available = self.inventory.projects.is_some();
        let mut largest_projects = self.inventory.projects.take().unwrap_or_default();
        largest_projects.sort_unstable_by(|a, b| {
            b.logical_bytes
                .cmp(&a.logical_bytes)
                .then_with(|| a.path.cmp(&b.path))
        });
        largest_projects.retain(|project| project.logical_bytes > 0);
        largest_projects.truncate(DISPLAY_LIMIT);
        Detail {
            entry,
            roots: self.inventory.roots,
            exclusions: self.inventory.exclusions,
            warnings: self.inventory.warnings,
            ecosystems: self.inventory.ecosystems,
            categories: self.inventory.categories,
            project_details_available,
            largest_projects,
        }
    }
}

fn bucket_changes(before: &Breakdown, after: &Breakdown) -> Vec<BucketChange> {
    before
        .keys()
        .chain(after.keys().filter(|key| !before.contains_key(*key)))
        .map(|name| BucketChange {
            name: name.clone(),
            before: before.get(name).copied().unwrap_or(0),
            after: after.get(name).copied().unwrap_or(0),
        })
        .collect()
}

impl ProjectChanges {
    fn ranked(mut projects: Vec<ProjectChange>) -> Self {
        let count = projects.len();
        let magnitude = |project: &ProjectChange| {
            project
                .after
                .unwrap_or(0)
                .abs_diff(project.before.unwrap_or(0))
        };
        projects.sort_unstable_by(|a, b| {
            magnitude(b)
                .cmp(&magnitude(a))
                .then_with(|| a.path.cmp(&b.path))
        });
        projects.truncate(DISPLAY_LIMIT);
        Self { count, projects }
    }
}

fn compare(before: &Snapshot, after: &Snapshot) -> Result<Comparison> {
    if !before.summary.complete || !after.summary.complete {
        return Err("Choose two complete inventories to compare.".into());
    }
    if before.summary.discovery_rule_version != after.summary.discovery_rule_version {
        return Err("These scans used different discovery rules. Run a fresh scan to start a comparable series.".into());
    }
    if before.summary.scope_key != after.summary.scope_key {
        return Err(
            "These scans used different folders, exclusions or protection settings.".into(),
        );
    }
    let mut comparison = Comparison {
        logical_bytes_delta: i128::from(after.summary.totals.logical_bytes)
            - i128::from(before.summary.totals.logical_bytes),
        ecosystems: bucket_changes(&before.inventory.ecosystems, &after.inventory.ecosystems),
        categories: bucket_changes(&before.inventory.categories, &after.inventory.categories),
        growth: None,
        discovered: None,
        no_longer_observed: None,
    };
    if let (Some(before), Some(after)) = (&before.inventory.projects, &after.inventory.projects) {
        let mut previous: BTreeMap<_, _> = before
            .iter()
            .map(|p| (p.path.as_path(), p.logical_bytes))
            .collect();
        let mut growth = Vec::new();
        let mut discovered = Vec::new();
        for project in after {
            match previous.remove(project.path.as_path()) {
                Some(bytes) if project.logical_bytes > bytes => growth.push(ProjectChange {
                    path: project.path.clone(),
                    before: Some(bytes),
                    after: Some(project.logical_bytes),
                }),
                None => discovered.push(ProjectChange {
                    path: project.path.clone(),
                    before: None,
                    after: Some(project.logical_bytes),
                }),
                _ => {}
            }
        }
        comparison.growth = Some(ProjectChanges::ranked(growth));
        comparison.discovered = Some(ProjectChanges::ranked(discovered));
        comparison.no_longer_observed = Some(ProjectChanges::ranked(
            previous
                .into_iter()
                .map(|(path, bytes)| ProjectChange {
                    path: path.to_path_buf(),
                    before: Some(bytes),
                    after: None,
                })
                .collect(),
        ));
    }
    Ok(comparison)
}

pub struct HistoryStore(pub PathBuf);

impl HistoryStore {
    fn file(&self) -> PathBuf {
        self.0.join("scan-history.sqlite3")
    }

    fn connect(&self) -> Result<Connection> {
        let mut directory = fs::DirBuilder::new();
        directory.recursive(true);
        let mut file_options = OpenOptions::new();
        file_options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
            directory.mode(0o700);
            file_options.mode(0o600);
        }
        directory.create(&self.0).map_err(|e| e.to_string())?;
        let path = self.file();
        #[cfg(unix)]
        // System paths such as macOS /var may be aliases. Resolve only the
        // trusted app directory, preserving the database's no-follow check.
        let path = self
            .0
            .canonicalize()
            .map_err(|e| e.to_string())?
            .join(path.file_name().expect("the history file has a fixed name"));
        match file_options.open(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.to_string()),
        }
        let metadata = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
        if !metadata.file_type().is_file() || metadata.len() > MAX_DATABASE_BYTES {
            return Err("Scan history storage is invalid or exceeds its size limit.".into());
        }
        let mut db = sql(Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        ))?;
        sql(db.busy_timeout(Duration::from_secs(4)))?;
        db.set_limit(
            Limit::SQLITE_LIMIT_LENGTH,
            (MAX_INVENTORY_BYTES + 64 * 1024) as i32,
        )
        .map_err(|error| error.to_string())?;
        sql(db.execute_batch(
            "PRAGMA trusted_schema = OFF;
             PRAGMA secure_delete = ON;
             PRAGMA synchronous = FULL;",
        ))?;
        migrate(&mut db)?;
        // These are 4 KiB pages, including SQLite bookkeeping. Rollback journals
        // are transient and may use additional space during a transaction.
        let page_size: i64 = sql(db.pragma_query_value(None, "page_size", |r| r.get(0)))?;
        if page_size != 4096 {
            return Err("Scan history uses an unsupported page size.".into());
        }
        sql(db.pragma_update(
            None,
            "max_page_count",
            MAX_DATABASE_BYTES as i64 / page_size,
        ))?;
        Ok(db)
    }

    pub fn list(&self) -> Result<HistoryIndex> {
        let mut db = self.connect()?;
        let transaction = sql(db.transaction())?;
        let retention = retention(&transaction)?;
        let mut query = sql(transaction.prepare(
            "SELECT id, summary FROM scans ORDER BY completed_at_ms DESC, id DESC LIMIT ?1",
        ))?;
        let mut rows = sql(query.query([MAX_RETENTION]))?;
        let mut entries = Vec::new();
        while let Some(row) = sql(rows.next())? {
            entries.push(Entry {
                id: sql(row.get(0))?,
                summary: decode_summary(
                    sql(row.get_ref(1))?.as_blob().map_err(|e| e.to_string())?,
                )?,
            });
        }
        Ok(HistoryIndex { retention, entries })
    }

    pub fn get(&self, id: i64) -> Result<Snapshot> {
        snapshot(&self.connect()?, id)
    }

    pub fn detail(&self, id: i64) -> Result<Detail> {
        Ok(self.get(id)?.detail(id))
    }

    pub fn compare(&self, before_id: i64, after_id: i64) -> Result<Comparison> {
        let mut db = self.connect()?;
        let transaction = sql(db.transaction())?;
        let before = snapshot(&transaction, before_id)?;
        let after = snapshot(&transaction, after_id)?;
        if (before.summary.completed_at_ms, before_id) >= (after.summary.completed_at_ms, after_id)
        {
            return Err("Choose an earlier scan as the comparison baseline.".into());
        }
        compare(&before, &after)
    }

    pub fn record(&self, mut snapshot: Snapshot) -> Result<()> {
        let summary = encode(&snapshot.summary, 16 * 1024)?;
        let inventory = match encode(&snapshot.inventory, MAX_INVENTORY_BYTES) {
            Ok(bytes) => bytes,
            Err(_) if snapshot.inventory.projects.is_some() => {
                snapshot.inventory.projects = None;
                encode(&snapshot.inventory, MAX_INVENTORY_BYTES)?
            }
            Err(error) => return Err(error),
        };

        self.update(|transaction| {
            sql(transaction.execute(
                "INSERT INTO scans (completed_at_ms, summary, inventory) VALUES (?1, ?2, ?3)",
                params![snapshot.summary.completed_at_ms, summary, inventory],
            ))?;
            prune(transaction, retention(transaction)?)
        })
    }

    pub fn remove(&self, id: i64) -> Result<()> {
        self.update(|tx| {
            sql(tx.execute("DELETE FROM scans WHERE id = ?1", [id]))?;
            Ok(())
        })
    }

    pub fn clear(&self) -> Result<()> {
        self.update(|tx| {
            sql(tx.execute("DELETE FROM scans", []))?;
            Ok(())
        })
    }

    pub fn set_retention(&self, count: u32) -> Result<()> {
        if !(1..=MAX_RETENTION).contains(&count) {
            return Err(format!(
                "Choose a retention limit from 1 to {MAX_RETENTION}."
            ));
        }
        self.update(|tx| {
            sql(tx.execute(
                "UPDATE preferences SET retention = ?1 WHERE id = 1",
                [count],
            ))?;
            prune(tx, count)
        })
    }

    fn update(&self, change: impl FnOnce(&Transaction<'_>) -> Result<()>) -> Result<()> {
        let mut db = self.connect()?;
        let transaction = sql(db.transaction_with_behavior(TransactionBehavior::Immediate))?;
        change(&transaction)?;
        sql(transaction.commit())
    }
}

fn migrate(db: &mut Connection) -> Result<()> {
    let version =
        |db: &Connection| sql(db.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0)));
    let empty_schema = |db: &Connection| {
        sql(
            db.query_row("SELECT NOT EXISTS (SELECT 1 FROM sqlite_schema)", [], |r| {
                r.get::<_, bool>(0)
            }),
        )
    };
    if version(db)? == 0 {
        // SQLite fixes its page layout when the first write transaction begins.
        if empty_schema(db)? {
            sql(db.execute_batch("PRAGMA page_size = 4096; PRAGMA auto_vacuum = FULL;"))?;
        }
        let transaction = sql(db.transaction_with_behavior(TransactionBehavior::Immediate))?;
        if version(&transaction)? == 0 {
            if !empty_schema(&transaction)? {
                return Err(
                    "Scan history uses an unrecognized database. The file was preserved.".into(),
                );
            }
            sql(transaction.execute_batch(&format!(
                "CREATE TABLE preferences (
                     id INTEGER PRIMARY KEY CHECK (id = 1),
                     retention INTEGER NOT NULL CHECK (retention BETWEEN 1 AND {MAX_RETENTION})
                 );
                 INSERT INTO preferences VALUES (1, {DEFAULT_RETENTION});
                 CREATE TABLE scans (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     completed_at_ms INTEGER NOT NULL,
                     summary BLOB NOT NULL CHECK (length(summary) <= 16384),
                     inventory BLOB NOT NULL CHECK (length(inventory) <= {MAX_INVENTORY_BYTES})
                 );
                 CREATE INDEX scans_recency ON scans (completed_at_ms DESC, id DESC);
                 PRAGMA application_id = {APPLICATION_ID};
                 PRAGMA user_version = {SCHEMA_VERSION};",
            )))?;
        }
        sql(transaction.commit())?;
    }
    if version(db)? != SCHEMA_VERSION {
        return Err("This scan history schema is unsupported. The database was preserved.".into());
    }
    if sql(db.pragma_query_value(None, "application_id", |r| r.get::<_, i64>(0)))? != APPLICATION_ID
    {
        return Err("Scan history uses an unrecognized database. The file was preserved.".into());
    }
    Ok(())
}

fn retention(db: &Connection) -> Result<u32> {
    let count = sql(db.query_row(
        "SELECT retention FROM preferences WHERE id = 1",
        [],
        |row| row.get(0),
    ))?;
    if !(1..=MAX_RETENTION).contains(&count) {
        return Err("Scan history retention data is invalid.".into());
    }
    Ok(count)
}

fn prune(tx: &Transaction<'_>, count: u32) -> Result<()> {
    let mut query = sql(tx.prepare(
        "SELECT id, completed_at_ms, length(summary) + length(inventory)
         FROM scans ORDER BY completed_at_ms DESC, id DESC",
    ))?;
    let mut rows = sql(query.query([]))?;
    let mut total = 0i64;
    let mut remaining = count;
    let mut cutoff = None;
    while let Some(row) = sql(rows.next())? {
        total += sql(row.get::<_, i64>(2))?;
        if remaining == 0 || total > MAX_PAYLOAD_BYTES as i64 {
            cutoff = Some((sql(row.get::<_, i64>(1))?, sql(row.get::<_, i64>(0))?));
            break;
        }
        remaining -= 1;
    }
    drop(rows);
    drop(query);
    if let Some((completed_at, id)) = cutoff {
        sql(tx.execute(
            "DELETE FROM scans WHERE (completed_at_ms, id) <= (?1, ?2)",
            params![completed_at, id],
        ))?;
    }
    Ok(())
}

fn snapshot(db: &Connection, id: i64) -> Result<Snapshot> {
    let (summary, inventory): (Vec<u8>, Vec<u8>) = sql(db
        .query_row(
            "SELECT summary, inventory FROM scans WHERE id = ?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional())?
    .ok_or("This saved scan is no longer in history.")?;
    Ok(Snapshot {
        summary: decode_summary(&summary)?,
        inventory: decode(&inventory)?,
    })
}

fn sql<T>(result: rusqlite::Result<T>) -> Result<T> {
    result.map_err(|error| format!("Scan history is unavailable: {error}"))
}

fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T> {
    serde_json::from_slice(bytes)
        .map_err(|error| format!("Scan history contains an invalid record: {error}"))
}

fn decode_summary(bytes: &[u8]) -> Result<Summary> {
    let summary: Summary = decode(bytes)?;
    // The desktop date formatter accepts the ECMAScript Date range.
    if !(0..=8_640_000_000_000_000).contains(&summary.completed_at_ms) {
        return Err("Scan history contains an invalid completion date.".into());
    }
    Ok(summary)
}

fn encode(value: &impl Serialize, limit: usize) -> Result<Vec<u8>> {
    struct Bounded(Vec<u8>, usize);
    impl Write for Bounded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if bytes.len() > self.1 - self.0.len() {
                return Err(io::Error::other("inventory exceeds its storage limit"));
            }
            self.0.write(bytes)
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    let mut output = Bounded(Vec::new(), limit);
    serde_json::to_writer(&mut output, value).map_err(|e| e.to_string())?;
    Ok(output.0)
}

#[cfg(test)]
mod tests;
