//! Private integration-test binary. No library, binary target or feature exports an executor.
#![cfg(target_os = "linux")]

use fs2::FileExt;
use rusqlite::{params, Connection};
use std::{
    error::Error,
    ffi::CStr,
    fs::{self, File, OpenOptions},
    io::{self, Read},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, OpenOptionsExt},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tempfile::TempDir;
use uuid::Uuid;

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const CONTENT: &[u8] = b"StackRinse disposable cache fixture v1\n";
const SLOT: &CStr = c"fixture.cache";
const TTL: Duration = Duration::from_secs(30);
const RULE: u32 = stackrinse_core::DISCOVERY_RULE_VERSION;

fn require(ok: bool, reason: &'static str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(reason.into())
    }
}

fn id(file: &File) -> io::Result<String> {
    let meta = file.metadata()?;
    Ok(format!("{}:{}", meta.dev(), meta.ino()))
}

fn directory(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(path)
}

struct Fixture {
    root: TempDir,
    generation: i64,
    protected: bool,
}

struct Plan {
    scan: Uuid,
    generation: i64,
    plan: Uuid,
    operation: Uuid,
    issued: Instant,
    created_ms: u128,
    // Keep the original inode alive until execution finishes.
    object: File,
    parents: [String; 2],
    original: PathBuf,
}

// Neither type can be deserialized. Approval consumes one exact plan.
struct Approved(Plan);

impl Plan {
    fn approve(self) -> Approved {
        Approved(self)
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Stop {
    Never,
    AfterIntent,
    AfterMove,
}

impl Fixture {
    fn new() -> Result<Self> {
        let root = tempfile::Builder::new()
            .permissions(std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .tempdir()?;
        for name in ["source", "vault"] {
            fs::create_dir(root.path().join(name))?;
        }
        fs::write(root.path().join("source/fixture.cache"), CONTENT)?;
        File::open(root.path().join("source/fixture.cache"))?.sync_all()?;
        directory(&root.path().join("source"))?.sync_all()?;
        File::create_new(root.path().join("instance.lock"))?;
        let f = Self {
            root,
            generation: 0,
            protected: false,
        };
        let db = Connection::open(f.root.path().join("operations.sqlite3"))?;
        db.execute_batch("PRAGMA synchronous=FULL;")?;
        db.execute_batch("BEGIN IMMEDIATE;
            CREATE TABLE item(op TEXT PRIMARY KEY, plan TEXT UNIQUE NOT NULL, scan TEXT NOT NULL,
                plan_version INTEGER NOT NULL CHECK(plan_version=1), generation INTEGER NOT NULL, rule INTEGER NOT NULL, created_ms TEXT NOT NULL, ttl_ms INTEGER NOT NULL,
                original TEXT NOT NULL, destination TEXT NOT NULL, source_parent TEXT NOT NULL, vault_parent TEXT NOT NULL,
                identity TEXT NOT NULL, bytes INTEGER NOT NULL, evidence TEXT NOT NULL, checks TEXT NOT NULL);
            CREATE TABLE event(seq INTEGER PRIMARY KEY, op TEXT NOT NULL REFERENCES item(op), state TEXT NOT NULL, detail TEXT NOT NULL);
            PRAGMA user_version=1; COMMIT;")?;
        directory(f.root.path())?.sync_all()?;
        Ok(f)
    }
    fn db(&self) -> Result<Connection> {
        let db = Connection::open_with_flags(
            self.root.path().join("operations.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )?;
        db.execute_batch("PRAGMA synchronous=FULL; PRAGMA foreign_keys=ON;")?;
        Ok(db)
    }
    fn lock(&self) -> Result<File> {
        let lock = File::open(self.root.path().join("instance.lock"))?;
        lock.try_lock_exclusive()?;
        Ok(lock)
    }
    fn dirs(&self) -> Result<[File; 2]> {
        Ok([
            directory(&self.root.path().join("source"))?,
            directory(&self.root.path().join("vault"))?,
        ])
    }
    fn policy(&self) -> Result<()> {
        require(!self.protected, "protection enabled")?;
        for dir in self.root.path().ancestors() {
            match fs::symlink_metadata(dir.join(".git")) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
                Ok(_) => return Err("repository boundary".into()),
            }
        }
        for location in ["source", "vault"] {
            for entry in fs::read_dir(self.root.path().join(location))? {
                require(
                    entry?.file_name() == "fixture.cache",
                    "unrecognized entry or repository",
                )?;
            }
        }
        Ok(())
    }
    fn scan(&mut self) -> Result<Plan> {
        self.generation += 1;
        let issued = Instant::now();
        self.policy()?;
        let dirs = self.dirs()?;
        let object = inspect(&dirs[0])?.ok_or("missing fixture")?;
        require(inspect(&dirs[1])?.is_none(), "occupied quarantine")?;
        Ok(Plan {
            scan: Uuid::new_v4(),
            generation: self.generation,
            plan: Uuid::new_v4(),
            operation: Uuid::new_v4(),
            issued,
            created_ms: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis(),
            original: self.root.path().join("source/fixture.cache"),
            object,
            parents: [id(&dirs[0])?, id(&dirs[1])?],
        })
    }
    fn execute(&self, Approved(plan): Approved, stop: Stop) -> Result<()> {
        let _guard = self.lock()?;
        let mut db = self.db()?;
        let op = plan.operation.to_string();
        check_schema(&db)?;
        let tx = db.transaction()?;
        tx.execute(
            "INSERT INTO item VALUES(?,?,?,1,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![
                op,
                plan.plan.to_string(),
                plan.scan.to_string(),
                plan.generation,
                RULE,
                plan.created_ms.to_string(),
                TTL.as_millis() as i64,
                plan.original.to_str().ok_or("path encoding")?,
                self.root
                    .path()
                    .join("vault/fixture.cache")
                    .to_str()
                    .ok_or("path encoding")?,
                plan.parents[0],
                plan.parents[1],
                id(&plan.object)?,
                CONTENT.len() as i64,
                "synthetic rebuildable cache v1; exact bytes",
                "exclusive fixture; policy; parents; identity; single link; empty destination"
            ],
        )?;
        event(&tx, &op, "authorized", "single-use fixture plan")?;
        tx.commit()?;
        let checked = (|| {
            let unresolved: bool = db.query_row(
                "SELECT EXISTS(SELECT 1 FROM event e WHERE op<>? AND seq=(SELECT max(seq) FROM event WHERE op=e.op)
                    AND state NOT IN ('refused','restored','interrupted'))", [&op], |r| r.get(0))?;
            require(!unresolved, "previous operation needs recovery")?;
            require(
                plan.generation == self.generation && plan.issued.elapsed() < TTL,
                "stale plan",
            )?;
            self.validate(&db, &op, false)
        })();
        self.move_checked(&db, &op, false, checked, stop)
    }
    fn validate(&self, db: &Connection, op: &str, restoring: bool) -> Result<[File; 2]> {
        let (dirs, slots) = self.observe(db, op)?;
        require(
            slots
                == if restoring {
                    [false, true]
                } else {
                    [true, false]
                },
            "conflicting slots",
        )?;
        Ok(dirs)
    }
    fn observe(&self, db: &Connection, op: &str) -> Result<([File; 2], [bool; 2])> {
        self.policy()?;
        let dirs = self.dirs()?;
        let (a, b, object): (String, String, String) = db.query_row(
            "SELECT source_parent,vault_parent,identity FROM item WHERE op=?",
            [op],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        require(id(&dirs[0])? == a && id(&dirs[1])? == b, "parent replaced")?;
        require(
            dirs[0].metadata()?.dev() == dirs[1].metadata()?.dev(),
            "cross-volume",
        )?;
        let mut slots = [false; 2];
        for (i, dir) in dirs.iter().enumerate() {
            if let Some(file) = inspect(dir)? {
                require(id(&file)? == object, "object replaced")?;
                slots[i] = true;
            }
        }
        Ok((dirs, slots))
    }
    fn restore(&self, op: &str, stop: Stop) -> Result<()> {
        let _guard = self.lock()?;
        let db = self.db()?;
        let state = state(&db, op)?;
        if !matches!(
            state.as_str(),
            "quarantined" | "restore_refused" | "restore_interrupted"
        ) {
            event(&db, op, &state, "restore refused in this state")?;
            return Err("restore refused in this state".into());
        }
        self.move_checked(&db, op, true, self.validate(&db, op, true), stop)
    }
    fn move_checked(
        &self,
        db: &Connection,
        op: &str,
        restore: bool,
        checked: Result<[File; 2]>,
        stop: Stop,
    ) -> Result<()> {
        let dirs = match checked {
            Ok(dirs) => dirs,
            Err(e) => {
                event(
                    db,
                    op,
                    if restore {
                        "restore_refused"
                    } else {
                        "refused"
                    },
                    &e.to_string(),
                )?;
                return Err(e);
            }
        };
        event(
            db,
            op,
            if restore {
                "restore_intent"
            } else {
                "move_intent"
            },
            "validated both slots",
        )?;
        if stop == Stop::AfterIntent {
            return Err("simulated interruption".into());
        }
        let src = usize::from(restore);
        // SAFETY: both descriptors remain open; SLOT is a static single C component.
        let rc = unsafe {
            libc::renameat2(
                dirs[src].as_raw_fd(),
                SLOT.as_ptr(),
                dirs[1 - src].as_raw_fd(),
                SLOT.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        if rc != 0 {
            let err = io::Error::last_os_error();
            event(
                db,
                op,
                if restore {
                    "restore_intent"
                } else {
                    "move_intent"
                },
                &err.to_string(),
            )?;
            return Err(err.into());
        }
        if stop == Stop::AfterMove {
            return Err("simulated interruption".into());
        }
        self.finish(db, op, restore)
    }
    fn finish(&self, db: &Connection, op: &str, restore: bool) -> Result<()> {
        let (dirs, slots) = match self.observe(db, op) {
            Ok(pair) => pair,
            Err(e) => {
                event(db, op, "ambiguous", &e.to_string())?;
                return Err(e);
            }
        };
        for dir in dirs {
            dir.sync_all()?;
        }
        let next = match (restore, slots) {
            (false, [false, true]) => "quarantined",
            (true, [true, false]) => "restored",
            (false, [true, false]) => "interrupted",
            (true, [false, true]) => "restore_interrupted",
            _ => "ambiguous",
        };
        event(db, op, next, "verified source and vault after intent")?;
        require(
            matches!(next, "restored" | "quarantined"),
            "movement incomplete",
        )
    }
    fn reconcile(&self, op: &str) -> Result<()> {
        let _guard = self.lock()?;
        let db = self.db()?;
        match state(&db, op)?.as_str() {
            "authorized" => event(&db, op, "interrupted", "no durable move intent"),
            "move_intent" => self.finish(&db, op, false),
            "restore_intent" => self.finish(&db, op, true),
            _ => Ok(()),
        }
    }
}

fn inspect(dir: &File) -> Result<Option<File>> {
    use std::os::fd::FromRawFd;
    // SAFETY: live directory fd and static NUL-terminated basename; transfer ownership once.
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            SLOT.as_ptr(),
            libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK,
        )
    };
    if fd < 0 {
        let e = io::Error::last_os_error();
        if e.kind() == io::ErrorKind::NotFound {
            return Ok(None);
        }
        return Err(e.into());
    }
    let file = unsafe { File::from_raw_fd(fd) };
    let meta = file.metadata()?;
    require(
        meta.dev() == dir.metadata()?.dev()
            && meta.is_file()
            && meta.nlink() == 1
            && meta.len() == CONTENT.len() as u64,
        "invalid fixture metadata",
    )?;
    let mut bytes = Vec::new();
    (&file)
        .take(CONTENT.len() as u64 + 1)
        .read_to_end(&mut bytes)?;
    require(bytes == CONTENT, "fixture content changed")?;
    Ok(Some(file))
}
fn check_schema(db: &Connection) -> Result<()> {
    require(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))? == 1,
        "unsupported journal",
    )
}
fn event(db: &Connection, op: &str, state: &str, detail: &str) -> Result<()> {
    db.execute(
        "INSERT INTO event(op,state,detail) VALUES(?,?,?)",
        params![op, state, detail],
    )?;
    Ok(())
}
fn state(db: &Connection, op: &str) -> Result<String> {
    check_schema(db)?;
    Ok(db.query_row(
        "SELECT state FROM event WHERE op=? ORDER BY seq DESC LIMIT 1",
        [op],
        |r| r.get(0),
    )?)
}

#[test]
fn durable_intents_reconcile_in_both_directions() -> Result<()> {
    for restore in [false, true] {
        for stop in [Stop::AfterIntent, Stop::AfterMove] {
            let mut f = Fixture::new()?;
            let plan = f.scan()?;
            let op = plan.operation.to_string();
            if restore {
                f.execute(plan.approve(), Stop::Never)?;
                assert!(f.restore(&op, stop).is_err());
            } else {
                assert!(f.execute(plan.approve(), stop).is_err());
            }
            let db = f.db()?;
            let pending:String=db.query_row("SELECT op FROM event WHERE state IN ('move_intent','restore_intent') AND seq=(SELECT MAX(seq) FROM event)",[],|r|r.get(0))?;
            assert_eq!(pending, op);
            assert_eq!(f.reconcile(&pending).is_ok(), stop == Stop::AfterMove);
            let expected = match (restore, stop) {
                (true, Stop::AfterIntent) => "restore_interrupted",
                (false, Stop::AfterIntent) => "interrupted",
                (true, _) => "restored",
                (false, _) => "quarantined",
            };
            assert_eq!(state(&db, &op)?, expected);
            if matches!(expected, "restore_interrupted" | "quarantined") {
                f.restore(&op, Stop::Never)?;
            }
            assert_eq!(
                fs::read(f.root.path().join("source/fixture.cache"))?,
                CONTENT
            );
            assert!(f.restore(&op, Stop::Never).is_err());
        }
    }
    Ok(())
}

#[test]
fn authorization_refuses_changed_evidence() -> Result<()> {
    for change in [
        "ancestor",
        "symlink",
        "object",
        "content",
        "hardlink",
        "repository",
        "worktree",
        "protected",
        "generation",
        "expired",
        "collision",
        "missing",
    ] {
        let mut f = Fixture::new()?;
        let mut plan = f.scan()?;
        let op = plan.operation.to_string();
        let src = f.root.path().join("source");
        let file = src.join("fixture.cache");
        let saved = f.root.path().join("saved");
        match change {
            "ancestor" => {
                fs::rename(&src, &saved)?;
                fs::create_dir(&src)?;
                fs::write(&file, CONTENT)?;
            }
            "symlink" => {
                fs::rename(&file, &saved)?;
                std::os::unix::fs::symlink(&saved, &file)?;
            }
            "object" => {
                fs::rename(&file, &saved)?;
                fs::write(&file, CONTENT)?;
            }
            "content" => fs::write(&file, vec![0; CONTENT.len()])?,
            "hardlink" => fs::hard_link(&file, &saved)?,
            "repository" => fs::create_dir(src.join(".git"))?,
            "worktree" => fs::write(src.join(".git"), b"gitdir: elsewhere")?,
            "protected" => f.protected = true,
            "generation" => {
                f.scan()?;
            }
            "expired" => plan.issued -= TTL,
            "collision" => fs::write(f.root.path().join("vault/fixture.cache"), b"existing")?,
            "missing" => fs::rename(&file, &saved)?,
            _ => unreachable!(),
        }
        let before = fs::read(&file).ok();
        assert!(f.execute(plan.approve(), Stop::Never).is_err(), "{change}");
        assert_eq!(state(&f.db()?, &op)?, "refused", "{change}");
        assert_eq!(fs::read(&file).ok(), before, "{change}");
        let vault = f.root.path().join("vault/fixture.cache");
        if change == "collision" {
            assert_eq!(fs::read(vault)?, b"existing");
        } else {
            assert!(!vault.try_exists()?);
        }
    }
    Ok(())
}

#[test]
fn conflicting_restoration_preserves_both_slots() -> Result<()> {
    let mut f = Fixture::new()?;
    let plan = f.scan()?;
    let op = plan.operation.to_string();
    let original = plan.original.clone();
    f.execute(plan.approve(), Stop::Never)?;
    fs::write(&original, b"user data")?;
    assert!(f.restore(&op, Stop::Never).is_err());
    assert_eq!(state(&f.db()?, &op)?, "restore_refused");
    assert_eq!(fs::read(original)?, b"user data");
    assert_eq!(
        fs::read(f.root.path().join("vault/fixture.cache"))?,
        CONTENT
    );
    Ok(())
}

#[test]
fn lock_and_failed_intent_prevent_movement() -> Result<()> {
    let mut f = Fixture::new()?;
    let plan = f.scan()?;
    let lock = f.lock()?;
    assert!(f.execute(plan.approve(), Stop::Never).is_err());
    drop(lock);
    let plan = f.scan()?;
    let op = plan.operation.to_string();
    f.db()?.execute_batch("CREATE TRIGGER fail BEFORE INSERT ON event WHEN NEW.state='move_intent' BEGIN SELECT RAISE(ABORT,'injected intent failure'); END;")?;
    assert!(f.execute(plan.approve(), Stop::Never).is_err());
    assert_eq!(state(&f.db()?, &op)?, "authorized");
    f.reconcile(&op)?;
    assert_eq!(state(&f.db()?, &op)?, "interrupted");
    assert_eq!(
        fs::read(f.root.path().join("source/fixture.cache"))?,
        CONTENT
    );
    Ok(())
}

#[test]
fn interrupted_with_conflicting_evidence_is_ambiguous() -> Result<()> {
    let mut f = Fixture::new()?;
    let p = f.scan()?;
    let op = p.operation.to_string();
    assert!(f.execute(p.approve(), Stop::AfterMove).is_err());
    fs::write(f.root.path().join("source/fixture.cache"), CONTENT)?;
    assert!(f.reconcile(&op).is_err());
    assert_eq!(state(&f.db()?, &op)?, "ambiguous");
    assert!(f.restore(&op, Stop::Never).is_err());
    Ok(())
}

#[test]
fn incomplete_inventory_invalidates_prior_approval_and_unsettled_intent_blocks_new_moves(
) -> Result<()> {
    let mut f = Fixture::new()?;
    let p = f.scan()?;
    let unexpected = f.root.path().join("source/unrecognized");
    fs::write(&unexpected, b"user")?;
    assert!(f.scan().is_err());
    fs::rename(unexpected, f.root.path().join("preserved"))?;
    assert!(f.execute(p.approve(), Stop::Never).is_err());
    let p = f.scan()?;
    let pending = p.operation.to_string();
    assert!(f.execute(p.approve(), Stop::AfterIntent).is_err());
    let other = f.scan()?;
    assert!(f.execute(other.approve(), Stop::Never).is_err());
    assert!(f.reconcile(&pending).is_err());
    let p = f.scan()?;
    f.execute(p.approve(), Stop::Never)?;
    Ok(())
}
