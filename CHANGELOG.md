# Changelog

All notable changes to this project will be documented in this file. The format roughly follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

_Nothing yet._

## [0.2.1] - 2025-10-03

### Fixed

- Ensure inline snippets invoked with `run <lang> -c` inherit the parent standard input across all engines.
- Accept `-c/--code` and `-f/--file` flags immediately after the language selector without consuming user snippet arguments.
- Add regression coverage so `run python -c` continues to consume piped input in future releases.

## [0.2.0] - 2025-10-02

### Added

- `run --version` and `run -V` now print rich build metadata (author, homepage, repository, license, git commit, build target, timestamp, and `rustc` version).
- New `build.rs` script captures git information and build context so binaries embed accurate metadata.
- `scripts/install.sh` provides a cross-shell installer that downloads the latest release, installs `run`, and optionally updates the PATH; release archives bundle the helper automatically.
- Automated release workflow now generates changelog notes with git-cliff, installs gettext for manifest templating, and publishes the generated changelog as the GitHub release body.
- Repository-level `cliff.toml` config powers consistent changelog generation going forward.

## [0.1.0] - 2025-10-02

### Added

- Initial public release of `run` with universal multi-language runner and REPL.
- Support for inline snippets, file execution, and persistent sessions across 20+ language engines (Python, Bash, Rust, Go, C/C++, Java, TypeScript, Swift, and more).
- Automatic language detection helpers when `--lang` is omitted.
- REPL with language switching commands and persistent snippet history per engine.
- GitHub release workflow that builds signed binaries for Linux, macOS, and Windows and uploads artifacts.

[Unreleased]: https://github.com/Esubaalew/run/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Esubaalew/run/releases/tag/v0.2.1
[0.2.0]: https://github.com/Esubaalew/run/releases/tag/v0.2.0
[0.1.0]: https://github.com/Esubaalew/run/releases/tag/v0.1.0
