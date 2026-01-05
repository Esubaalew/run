# Changelog

All notable changes to this project will be documented in this file. The format roughly follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

Nothing yet.

## [0.3.2] - 2026-01-05

### Fixed

- REPL: Improve interactive multiline typing across languages (continue when the line is incomplete, not just when delimiters are unbalanced).
- Python REPL: Auto-indent the first line after `def/if/for/...:` headers to avoid `IndentationError` when typing blocks line-by-line.

## [0.3.1] - 2026-01-05

### Fixed

- C#: REPL now prints more expression forms (method calls, member access like `"Hello".Length`, ternary `?:`) and better handles trailing semicolons.
- C#: Improve REPL output for `null` and common collection results.
- Groovy: REPL now behaves closer to `groovysh` by printing tail expressions (including assignment expressions) and supporting more expression-y forms.
- TypeScript: REPL expression printing now tolerates a single trailing semicolon.
- REPL: Interactive multiline input is now supported (e.g. typing Python `def` blocks line-by-line) via a continuation prompt; use a blank line to finish blocks and `:cancel` to abort.

### Testing

- Add regression suites for C#, Groovy, and TypeScript REPL expression printing and semantics.

## [0.3.0] - 2025-10-31

### Added

- **Syntax highlighting** for all supported languages in the REPL with real-time color coding

  - Automatically adapts when switching languages

- Lua REPL support for `= expr` syntax to evaluate and print expressions

### Fixed

- Zig: File execution and session expression evaluation
- Nim: File execution and compiler message filtering
- Go: Standalone function execution with session imports
- Haskell: Variable scoping and `let` bindings in REPL sessions
- TypeScript: Color code handling in Deno output
- Improved error detection in C# and Kotlin REPL sessions

## [0.2.1] - 2025-10-10

### Fixed

- Preserve top-level import/package lines for Kotlin and Java wrapper flows (avoid imports inside generated main/class).
- Add TypeScript (Deno) guidance and Dart quoting guidance to README; recommend quoted here-docs for shell-sensitive snippets and provide zsh-safe inline examples.

## [0.2.0] - 2025-10-09

### Added

- Groovy language support via the `groovy` CLI, including inline, file, and stdin execution plus new sample scripts in `examples/groovy/`.

## [0.1.1] - 2025-10-04

### Changed

- Polished `README.md`: added a crates.io badge, fixed heading spacing, and corrected typos so the documentation shown on crates.io matches the repository.

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

[Unreleased]: https://github.com/Esubaalew/run/compare/v0.3.2...HEAD
[0.3.2]: https://github.com/Esubaalew/run/compare/v0.3.1...v0.3.2
[0.3.1]: https://github.com/Esubaalew/run/compare/v0.3.0...v0.3.1
[0.3.0]: https://github.com/Esubaalew/run/compare/v0.2.1...v0.3.0
[0.2.1]: https://github.com/Esubaalew/run/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Esubaalew/run/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/Esubaalew/run/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/Esubaalew/run/releases/tag/v0.1.0
