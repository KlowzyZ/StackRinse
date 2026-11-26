"""Generate disposable mixed-ecosystem fixtures using installed development tools."""
import argparse
import json
import os
from pathlib import Path
import subprocess


def write(root, path, text):
    destination = root / path
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(text, encoding="utf-8")


def manifest(root, path, value):
    write(root, path, json.dumps(value) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path, help="new directory for fixtures and isolated tool state")
    output = parser.parse_args().output.resolve()
    output.mkdir(parents=True)

    fixtures = output / "fixtures"
    state = output / "tool-state"
    env = dict(os.environ, PUB_CACHE=str(state / "pub"),
               COMPOSER_HOME=str(state / "composer"), COMPOSER_DISABLE_NETWORK="1",
               GRADLE_USER_HOME=str(state / "gradle"), BUN_INSTALL_CACHE_DIR=str(state / "bun"))

    def run(fixture, *command):
        subprocess.run(command, cwd=fixtures / fixture, env=env, check=True)

    dart = fixtures / "dart"
    write(dart, "pubspec.yaml", "name: storage_sample\nenvironment:\n  sdk: '>=3.0.0 <4.0.0'\n")
    write(dart, "lib/storage_sample.dart", "int value = 1;\n")
    run("dart", "dart", "pub", "get", "--offline")

    php = fixtures / "composer"
    manifest(php, "local-package/composer.json", {
        "name": "stackrinse/fixture-dependency", "version": "1.0.0",
        "type": "library", "autoload": {"files": ["lib.php"]},
    })
    write(php, "local-package/lib.php", "<?php function fixture_answer() { return 42; }\n")
    manifest(php, "composer.json", {
        "name": "stackrinse/storage-sample",
        "require": {"stackrinse/fixture-dependency": "1.0.0"},
        "repositories": [
            {"packagist.org": False},
            {"type": "path", "url": "local-package", "options": {"symlink": False}},
        ],
    })
    run("composer", "composer", "install", "--no-interaction", "--no-plugins", "--no-scripts")

    gradle = fixtures / "gradle"
    write(gradle, "settings.gradle", "rootProject.name = 'storage-sample'\n")
    write(gradle, "build.gradle", "plugins { id 'java' }\n")
    write(gradle, "src/main/java/Sample.java",
          "public final class Sample { public int answer() { return 42; } }\n")
    run("gradle", "gradle", "--no-daemon", "--offline", "build")

    python = fixtures / "python"
    write(python, "pyproject.toml", '[project]\nname = "storage_sample"\nversion = "1.0.0"\n')
    write(python, "sample.py", "def answer() -> int:\n    return 42\n")
    run("python", "ruff", "check", ".")
    run("python", "mypy", "sample.py")

    bun = fixtures / "bun"
    manifest(bun, "local-package/package.json", {
        "name": "fixture-dependency", "version": "1.0.0", "main": "index.js",
    })
    write(bun, "local-package/index.js", "module.exports = 42;\n")
    manifest(bun, "package.json", {
        "name": "storage-sample", "private": True,
        "dependencies": {"fixture-dependency": "file:./local-package"},
    })
    run("bun", "bun", "install", "--linker", "isolated", "--ignore-scripts")


if __name__ == "__main__":
    main()
