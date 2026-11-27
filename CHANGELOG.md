# Changelog

## [0.1.0-alpha.1]

First public alpha of the StackRinse desktop workbench and source-built CLI.

### Discovery and review

- Scan selected workspace roots for project-local dependencies, build outputs and caches. Browse projects, inspect detection evidence and rebuild implications, and review exact paths in a read-only dry run.
- Recognize supported layouts across Node.js, Python, Rust, Go, CMake, Godot, .NET, Maven, Gradle, Dart Pub and PHP Composer.
- Protect candidates at filesystem and Git boundaries, when evidence is mixed or incomplete, and when excluded by user preferences.

### Storage insights

- Break down observed logical bytes by artifact category or ecosystem, with reviewable and protected totals.
- Save scans locally and compare complete reports that share scan settings and discovery rules.
- Export the current inventory or a saved scan to JSON or CSV from the desktop.

### Distribution and limits

- Windows x64 ZIP with an unsigned desktop executable, embedded frontend, launch instructions, dependency notices, build provenance and SHA-256 verification. WebView2 Evergreen is required.
- Actual cleanup, quarantine, restoration and permanent deletion are unavailable. Measured files do not establish recoverable disk space.

[0.1.0-alpha.1]: https://github.com/KlowzyZ/StackRinse/releases/tag/v0.1.0-alpha.1
