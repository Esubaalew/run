# Changelog

All notable changes to this project will be documented in this file. The format roughly follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

_Nothing yet._

## [0.1.0] - 2025-10-03

### Added

- Initial public release of `run` with a universal multi-language runner and REPL.
- Support for inline snippets, file execution, and persistent sessions across 20+ language engines (Python, Bash, Rust, Go, C/C++, Java, TypeScript, Swift, and more).
- Automatic language detection helpers when `--lang` is omitted.
- REPL with language switching commands and persistent snippet history per engine.
- `run --version` / `run -V` print rich build metadata (author, homepage, repository, license, git commit, build target, timestamp, and `rustc` version).
- `scripts/install.sh` provides a cross-shell installer that downloads the latest release, installs `run`, and optionally updates the PATH.
- Automated release workflow powered by `cliff.toml` generates changelog notes and publishes them with each GitHub release.

### Fixed

- Inline snippets invoked with `run <lang> -c` inherit standard input correctly across all engines.
- `-c/--code` and `-f/--file` flags are accepted immediately after the language selector without consuming snippet text.
- Added regression coverage ensuring `run python -c` continues to consume piped input in future releases.

[Unreleased]: https://github.com/Esubaalew/run/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/Esubaalew/run/releases/tag/v0.1.0
