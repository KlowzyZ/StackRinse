# Tool-generated evidence excerpts

These files came from the disposable projects in the mixed benchmark. They are small enough to embed in portable Rust tests.

- Dart Pub 3.14.1 produced both JSON files from a minimal package. The machine-local Pub cache URI was removed.
- Composer 2.10.3 installed a local path package using mirror mode. The test inputs retain the installed-package metadata and the generated headers of both autoload files.
- Gradle 9.3.1 with JDK 21 produced the cleanup-cache metadata while compiling a Java project. The test inputs retain the Gradle version line and the first 32 bytes of the binary output index.
- Ruff 0.17.6 and mypy 2.4.1 wrote the cache tags and generated ignore files.

The tests use these same bytes in missing-marker, linked-marker and mixed-content cases. The full tool output is used by the mixed benchmark. See [fixture generation](../../../../../benchmarks/README.md).
