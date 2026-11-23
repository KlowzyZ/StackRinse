# Reading a scan safely

A scan starts at the roots you select. A match describes why a directory looks like a development artifact. Even a genuine dependency tree or compiler output can contain hand edits, generated source that cannot be reproduced locally, local databases, or the only copy of a build.

## Reading the numbers

Logical size counts file lengths. Sparse files, compression, hard links, snapshots and filesystem allocation can make that figure differ from physical usage. Summing candidate lengths is an inventory estimate. Free space requires a separate, reliable measurement. Moving an item to an operating-system Trash can leave disk usage unchanged.

A timestamp is evidence of filesystem activity. It cannot establish whether a developer still needs the project. Rebuild costs are qualitative. Low means local regeneration is usually straightforward, medium indicates setup or compilation, and high can involve substantial dependency installation or compilation. Unknown means evidence is insufficient to estimate the work. Environment availability, toolchain versions and network access affect all four.

## Protection boundary

The initial implementation offers inspection and dry-run review. It has no filesystem cleanup executor. This write boundary protects tracked files, untracked work and data that changes after discovery even if a detection rule is wrong. Scan selection, an ignored path or a recognized name never grants permission to remove a directory.

Paths and metadata can change between filesystem calls. A read-only report is a snapshot with possible gaps. Reusing a scan-time absolute path for recursive deletion would introduce a time-of-check/time-of-use race.

## Requirements for enabling writes

An executor needs a reviewed, immutable selection bound to object identities, a fresh check of each ancestor and candidate, repository ownership and tracked-content checks, and an operation-time boundary that survives replacement and rename. Each supported platform needs dedicated adversarial temporary fixtures. Refuse the operation where the platform cannot enforce the boundary.

Record intent before a mutation, then record each item's outcome. A failure or interruption can leave a partial result. Report planned logical bytes, data moved to Trash and confirmed space recovery separately. Advertise recovery only when restoration has been exercised on the relevant platform. Permanent deletion requires independent consent.
