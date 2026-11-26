# Recoverable operation experiment

## Security contract

The unit of movement is one synthetic, known-content regular cache file. A test creates a private temporary directory, a single-level source directory, and an adjacent quarantine directory. Neither directory is a repository or contains a project manifest. No input selects a root or supplies an object identity. Both names are fixed ASCII components. The harness assumes exclusive control of the temporary tree; another process running as its owner can violate this assumption. Its advisory instance lock coordinates cooperating writers only. No real discovery rule or historical record grants this capability.

Before movement, bind the live scan, policy generation, rule version, evidence, logical length, original location, parent identities and file identity to an immutable selection. Authorization consumes the reviewed selection. Refuse incomplete or cancelled inventory, expired selection, changed generation, protection, unexpected entries, hard links, changed bytes or identities. Refuse Git markers including worktree pointer files; even untracked developer content is outside this experiment. Never continue after an inspection error. Accept one item only; partial batches require a separately reviewed aggregation protocol.

A committed SQLite intent precedes each move. Reconciliation observes both slots. Exactly one matching object in the expected slot is necessary to confirm movement. An untouched source after interrupted intent permits a report of interruption, never an automatic retry. Both slots, neither slot, identity mismatch, and unreadable state require operator inspection. Restoration has its own intent, identity checks and exclusive destination. All moves stay on one volume without copy fallback. Moving bytes into quarantine generally recovers no physical space. No purge API exists.

## Platform qualification required

| Platform | Candidate primitives | Outstanding guarantees |
| --- | --- | --- |
| Linux | Open directory descriptors, O_NOFOLLOW, fstat device/inode/link count, renameat2 with RENAME_NOREPLACE | The rename resolves the source name at the instant of movement. A retained descriptor prevents inode reuse while open but cannot prevent concurrent replacement, ancestor relocation or content writes. Device numbers alone miss bind mounts. External-writer trees are unsupported. |
| Windows | CreateFileW with FILE_FLAG_BACKUP_SEMANTICS and FILE_FLAG_OPEN_REPARSE_POINT per component; GetFileInformationByHandleEx with FileIdInfo and FileAttributeTagInfo; SetFileInformationByHandle with FileRenameInfo and replacement disabled | Qualify share modes, directory-relative destination semantics, open-handle source rename, volume serial and 128-bit identifiers, junctions, mount points, case-sensitive directories, locks, ACLs, identifier reuse and durable rename. A final-component reparse check misses ancestor redirection. No Windows executor. |
| macOS | openat with O_NOFOLLOW, fstat and renameatx_np with RENAME_EXCL | Directory-relative operations still resolve a mutable source name. APFS case handling, mounts and crash durability require native qualification. No macOS executor. |

Case folding or canonicalization cannot establish identity. Require filesystem-enforced destination exclusivity. A cross-device error, sharing violation, read-only volume or access denial must preserve the remaining data. A live Git index or preference change can race a production move independently of pathname races. Reject until enforceable policy synchronization and repository ownership are demonstrated.

## Independent audit

Review production build targets, IPC permissions and CLI dispatch; trace consent through stale scans and interrupted operations. Repeat replacement and mount tests with independent writers on disposable volumes. Review journal corruption, power loss, filesystem flush ordering, directory identity after restart, concurrent clients, and manual recovery with both slots occupied. Qualify NTFS and each proposed filesystem separately. Require independent review and explicit maintainer approval before connecting any executor.

## Journal protocol v1

The integration test owns a separate operations.sqlite3 database inside the private fixture. It uses SQLite FULL synchronous commits, an append-only event sequence and a unique plan per one-item operation. The persistent record contains both locations, device/inode observations, rule and plan versions, scan and operation UUIDs, generation, checks, evidence, logical bytes and issuance time. The 30-second deadline uses monotonic elapsed time in the issuing process. Restart discards approval. An attempt cannot resume a pre-crash plan.

| Latest event | Permitted outcome |
| --- | --- |
| authorized | refused, move_intent, or interrupted on reopening before intent |
| move_intent | quarantined after verified movement and directory flushes, interrupted if the original is intact, otherwise ambiguous |
| quarantined, restore_refused, restore_interrupted | restore_intent after fresh validation or restore_refused |
| restore_intent | restored after verification and flushes, restore_interrupted if still quarantined, otherwise ambiguous |
| restored, refused, interrupted, ambiguous | no further movement under that operation |

Rename errors trigger verification of both slots. An intact source and empty destination yield failed or restore_failed; uncertain evidence yields ambiguous. A verified restore_failed item permits a fresh restore attempt. A failed completion commit also leaves the prior intent durable. A busy instance cannot safely acquire the journal writer role. Recovery evidence assumes fixture ownership continues across connection reopens. The tests model process interruption at commit boundaries, not media failure or full machine power loss. The fixture's synthetic file cannot match any directory-based discovery rule. A future real quarantine needs a scanner-enforced identity boundary that covers explicit scan roots, nested roots and aliases before storing any project data.

The [Sprint 7 audit](recovery-audit.md) updates the qualification evidence, journal checks and process interruption results.
