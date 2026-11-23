//! Terminal entry points for the shared, read-only storage engine.
use clap::{ArgAction, Args, Parser, Subcommand};
use serde::Serialize;
use stackrinse_core::{dry_run, scan, Classification, Cost, Report, ScanOptions};
use std::{
    io::{self, BufWriter, IsTerminal, Write},
    path::PathBuf,
    process::ExitCode,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};

#[derive(Parser)]
#[command(
    name = "stackrinse",
    version,
    about = "Clean builds. Keep code.",
    long_about = "Discover generated developer storage and preview cleanup locally. Choose one or more directories to scan. All commands are read-only.",
    after_help = "Examples:\n  stackrinse scan ./projects --min-size 100MB\n  stackrinse inspect ./projects --json\n  stackrinse clean ./projects --dry-run\n\nExit codes:\n  0 complete   1 operational error   2 invalid arguments\n  3 scan warnings   130 interrupted"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
    /// Emit machine-readable JSON on stdout.
    #[arg(long, global = true, conflicts_with = "quiet")]
    json: bool,
    /// Suppress normal output; diagnostics and exit status remain available.
    #[arg(short, long, global = true, conflicts_with = "verbose")]
    quiet: bool,
    /// Show scan progress and warnings on stderr.
    #[arg(short, long, global = true, action = ArgAction::SetTrue)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Command {
    /// Inventory projects and their candidate logical sizes.
    Scan {
        #[command(flatten)]
        scope: Scope,
        /// Minimum candidate size per project (bytes or KB/MB/GB/TB/KiB/MiB/GiB/TiB).
        #[arg(long, value_parser = parse_size, default_value = "0")]
        min_size: u64,
    },
    /// Show candidate paths, classification, evidence, and rebuild consequences.
    Inspect(Scope),
    /// Preview a cleanup plan. This build only supports dry runs.
    Clean {
        #[command(flatten)]
        scope: Scope,
        /// Explicitly request the default read-only preview.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Args)]
struct Scope {
    /// Explicit directory roots to inspect.
    #[arg(required = true, value_name = "ROOT", num_args = 1..)]
    roots: Vec<PathBuf>,
    /// Directory subtree to skip. May be repeated.
    #[arg(long, value_name = "DIRECTORY")]
    exclude: Vec<PathBuf>,
    /// Project to protect from planning. May be repeated.
    #[arg(long, value_name = "PROJECT")]
    protect: Vec<PathBuf>,
}

fn parse_size(s: &str) -> Result<u64, String> {
    let s = s.trim();
    let i = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let n: u64 = s[..i]
        .parse()
        .map_err(|_| "expected a whole number of bytes or a size such as 100MB")?;
    let unit = s[i..].trim().to_ascii_lowercase();
    let multiplier = match unit.as_str() {
        "" | "b" => 1,
        "kb" => 1000,
        "mb" => 1_000_000,
        "gb" => 1_000_000_000,
        "tb" => 1_000_000_000_000,
        "kib" => 1 << 10,
        "mib" => 1 << 20,
        "gib" => 1 << 30,
        "tib" => 1 << 40,
        _ => return Err("unknown size unit; try MB or MiB".into()),
    };
    n.checked_mul(multiplier)
        .ok_or_else(|| "size exceeds u64".into())
}

fn size(n: u64) -> String {
    let mut value = n as f64;
    for unit in ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"] {
        if value < 1024.0 {
            return if unit == "B" {
                format!("{n} B")
            } else {
                format!("{value:.1} {unit}")
            };
        }
        value /= 1024.0;
    }
    unreachable!()
}

// Filesystem-controlled names can contain terminal commands and embedded newlines.
fn terminal_text(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}') {
            output.extend(c.escape_default());
        } else {
            output.push(c);
        }
    }
    output
}

fn json(out: &mut impl Write, value: &impl Serialize) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut *out, value)?;
    writeln!(out)
}

fn inventory(out: &mut impl Write, report: &Report, details: bool) -> io::Result<()> {
    writeln!(
        out,
        "Projects: {}   ·   Directories inspected: {}",
        report.projects.len(),
        report.directories
    )?;
    writeln!(
        out,
        "Sizes below are logical candidate bytes. Physical disk use can differ.\n"
    )?;
    let totals = report.totals();
    writeln!(
        out,
        "Inventory: {} logical · {} reviewable · {} protected",
        size(totals.logical_bytes),
        size(totals.reviewable_bytes),
        size(totals.protected_bytes)
    )?;
    for project in &report.projects {
        let path = terminal_text(&project.path.to_string_lossy());
        writeln!(
            out,
            "{:>11}   {}   [{}]{}{}",
            size(project.logical_bytes),
            path,
            project.ecosystems.join(", "),
            if project.candidates.iter().any(|c| !c.size_complete) {
                "   partial size"
            } else {
                ""
            },
            if project.protected {
                "   protected"
            } else {
                ""
            }
        )?;
        if details {
            for c in &project.candidates {
                let classification = match c.classification {
                    Classification::Rebuildable => "Rebuildable",
                    Classification::ReviewRequired => "Review required",
                    Classification::Protected => "Protected",
                };
                let cost = match c.cost {
                    Cost::Low => "low",
                    Cost::Medium => "medium",
                    Cost::High => "high",
                    Cost::Unknown => "unknown",
                };
                writeln!(
                    out,
                    "    {} · {} · {} · {}",
                    terminal_text(&c.path.to_string_lossy()),
                    if c.size_complete {
                        size(c.logical_bytes)
                    } else {
                        format!("{} observed (partial)", size(c.logical_bytes))
                    },
                    classification,
                    c.category
                )?;
                writeln!(out, "      Evidence: {}", terminal_text(&c.reason))?;
                writeln!(
                    out,
                    "      Rebuild ({} cost): {}",
                    cost,
                    terminal_text(&c.rebuild)
                )?;
            }
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct Preview<P> {
    mode: &'static str,
    cancelled: bool,
    warnings: Vec<String>,
    plan: P,
}

fn resolve_directories(paths: &[PathBuf]) -> io::Result<Vec<PathBuf>> {
    paths
        .iter()
        .map(|path| {
            let canonical = path.canonicalize().map_err(|error| {
                io::Error::new(error.kind(), format!("{}: {error}", path.display()))
            })?;
            if !canonical.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("{} must be a directory", path.display()),
                ));
            }
            Ok(canonical)
        })
        .collect()
}

fn run(cli: Cli) -> io::Result<u8> {
    let scope = match &cli.command {
        Command::Scan { scope, .. } | Command::Clean { scope, .. } | Command::Inspect(scope) => {
            scope
        }
    };
    let cancel = Arc::new(AtomicBool::new(false));
    let interrupt = Arc::clone(&cancel);
    ctrlc::set_handler(move || interrupt.store(true, std::sync::atomic::Ordering::Relaxed))
        .map_err(io::Error::other)?;
    let options = ScanOptions {
        exclusions: resolve_directories(&scope.exclude)?,
        protected_projects: resolve_directories(&scope.protect)?,
    };
    let show_progress = cli.verbose && io::stderr().is_terminal();
    let progress_timer = std::sync::Mutex::new(None);
    let mut report = scan(&scope.roots, &options, &cancel, |p| {
        if show_progress {
            let mut timer = progress_timer.lock().expect("progress timer");
            let now = std::time::Instant::now();
            if timer.is_none_or(|prev| now.duration_since(prev) >= Duration::from_millis(200)) {
                eprint!(
                    "\rScanning · {} directories · {} projects",
                    p.directories, p.projects
                );
                *timer = Some(now);
            }
        }
    })
    .map_err(io::Error::other)?;
    if show_progress {
        eprintln!();
    }
    if report.cancelled {
        eprintln!("Scan cancelled. Results are partial.");
    }
    if !report.warnings.is_empty() {
        eprintln!(
            "Scan reported {} warning(s). See --verbose or --json for details.",
            report.warnings.len()
        );
        if cli.verbose {
            for warning in &report.warnings {
                eprintln!("  {}", terminal_text(warning));
            }
        }
    }
    let status = if report.cancelled {
        130
    } else if !report.warnings.is_empty() {
        3
    } else {
        0
    };
    if cli.quiet {
        return Ok(status);
    }
    let mut out = BufWriter::new(io::stdout().lock());
    if let Command::Scan { min_size, .. } = cli.command {
        report.projects.retain(|p| p.logical_bytes >= min_size);
    }
    report.projects.sort_by(|a, b| {
        b.logical_bytes
            .cmp(&a.logical_bytes)
            .then_with(|| a.path.cmp(&b.path))
    });
    if matches!(cli.command, Command::Clean { .. }) {
        let plan = dry_run(&report);
        if cli.json {
            json(
                &mut out,
                &Preview {
                    mode: "dry_run",
                    cancelled: report.cancelled,
                    warnings: report.warnings,
                    plan,
                },
            )?;
        } else {
            writeln!(
                out,
                "Dry run · {} candidates · {} logical\nNo files changed.",
                plan.candidates().len(),
                size(plan.logical_bytes())
            )?;
            for c in plan.candidates() {
                writeln!(
                    out,
                    "{:>11}   {}",
                    size(c.logical_bytes),
                    terminal_text(&c.path.to_string_lossy())
                )?;
            }
        }
    } else if cli.json {
        json(&mut out, &report)?;
    } else {
        inventory(
            &mut out,
            &report,
            matches!(cli.command, Command::Inspect(_)),
        )?;
    }
    out.flush()?;
    Ok(status)
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => ExitCode::from(code),
        Err(e) if e.kind() == io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("stackrinse: {}", terminal_text(&e.to_string()));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_support_decimal_and_binary_units_without_overflow() {
        assert_eq!(parse_size("100MB").unwrap(), 100_000_000);
        assert_eq!(parse_size("2 MiB").unwrap(), 2_097_152);
        assert_eq!(size(2048), "2.0 KiB");
        for invalid in ["-1", "", "1.5GB", "1PB", "18446744073709551615TiB"] {
            assert!(parse_size(invalid).is_err());
        }
    }

    #[test]
    fn terminal_paths_cannot_inject_escape_sequences() {
        assert!(!terminal_text("before\x1b[2J\nafter").contains(['\n', '\x1b']));
        assert_eq!(terminal_text("/tmp/日本語"), "/tmp/日本語");
    }
}
