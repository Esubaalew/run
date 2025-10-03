# run

<p align="center">
	<strong>Polyglot command runner & smart REPL that lets you script, compile, and iterate in 25+ languages without touching another CLI.</strong>
</p>

<p align="center">
	<a href="https://github.com/Esubaalew/run/actions/workflows/release.yml"><img src="https://github.com/Esubaalew/run/actions/workflows/release.yml/badge.svg" alt="Release pipeline" /></a>
	<a href="https://github.com/Esubaalew/run/releases/latest"><img src="https://img.shields.io/github/v/release/Esubaalew/run?display_name=tag&sort=semver" alt="Latest release" /></a>
	<a href="https://github.com/Esubaalew/run/releases"><img src="https://img.shields.io/github/downloads/Esubaalew/run/total.svg" alt="Downloads" /></a>
	<a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="License" /></a>
</p>

> Built in Rust for developers who live in multiple runtimes. `run` gives you a consistent CLI, persistent REPLs, and batteries-included examples for your favorite languages.

---

<details>
<summary><strong>Table of contents</strong></summary>

- [Highlights](#-highlights)
- [Quickstart](#-quickstart)
- [Installation](#-installation)
- [How it works](#-how-it-works)
- [Supported languages](#-supported-languages)
- [Examples](#-examples)
- [REPL cheat sheet](#-repl-cheat-sheet)
- [Extending run](#-extending-run)
- [Testing & quality](#testing--quality)
- [📄 License](#-license)

</details>

---

## Here is what run means

- **One command, many runtimes.** Switch between Python, Go, Rust, TypeScript, Zig, Haskell, and more without leaving the same shell session.
- **Stateful REPLs.** Every engine keeps session history, understands `:reset`, `:load`, and language shortcuts (`:py`, `:go`, …), and auto-detects snippets when you want it to.
- **Inline, files, or stdin.** Evaluate one-liners with `--code`, run files with detection heuristics, or pipe input from another process.
- **Production-ready binaries.** Release workflow ships signed archives, Homebrew and Scoop manifests, plus Debian packages straight from CI.
- **Extensible by design.** Drop in a new `LanguageEngine` implementation and wire it into the registry to make `run` speak yet another language.
- **Developer ergonomics.** Rich metadata (`run --version`), fast autocomplete-friendly subcommands, and examples for every supported runtime.

##  Quickstart

```bash
# Show build metadata for the current binary
run --version

# Execute a snippet explicitly
run --lang python --code "print('hello, polyglot world!')"

# Let run detect language from the file extension
run examples/go/hello/main.go

# Drop into the interactive REPL (type :help inside)
run

# Pipe stdin (here: JSON) into Node.js
echo '{"name":"Ada"}' | run js --code "const data = JSON.parse(require('fs').readFileSync(0, 'utf8')); console.log(`hi ${data.name}`)"
```

Pro tip: `run` aliases the first positional argument, so `run py script.py` works just like `run --lang python script.py`.

##  Installation

All release assets are published on the [GitHub Releases](https://github.com/Esubaalew/run/releases) page, including macOS builds for both Apple Silicon (arm64) and Intel (x86_64). Pick the method that fits your platform:

<details>
<summary><strong>Cargo (Rust)</strong></summary>

```bash
cargo install run-kit
```

> Installs the `run` binary from the [`run-kit`](https://crates.io/crates/run-kit) crate. Updating? Run `cargo install run-kit --force`.

</details>

<details>
<summary><strong>Homebrew (macOS)</strong></summary>

```bash
brew install --formula https://github.com/Esubaalew/run/releases/latest/download/homebrew-run.rb
```

>  This formula is published as a standalone file on each release; it isn’t part of the default Homebrew taps. Installing by name (`brew install homebrew-run`) will fail—always point Homebrew to the release URL above (or download the file and run `brew install ./homebrew-run.rb`).

Once the latest release artifacts are published, Homebrew automatically selects the correct macOS binary for your CPU (Intel or Apple Silicon) based on this formula.

</details>

<details>
<summary><strong>Debian / Ubuntu</strong></summary>

```bash
curl -LO https://github.com/Esubaalew/run/releases/latest/download/run-deb.sha256
DEB_FILE=$(awk '{print $2}' run-deb.sha256)
curl -LO "https://github.com/Esubaalew/run/releases/latest/download/${DEB_FILE}"
sha256sum --check run-deb.sha256
sudo apt install "./${DEB_FILE}"
```

</details>

<details>
<summary><strong>Windows (Scoop)</strong></summary>

```powershell
scoop install https://github.com/Esubaalew/run/releases/latest/download/run-scoop.json
```

</details>

<details>
<summary><strong>Install script (macOS / Linux)</strong></summary>

```bash
curl -fsSLO https://raw.githubusercontent.com/Esubaalew/run/master/scripts/install.sh
chmod +x install.sh
./install.sh --add-path           # optional: append ~/.local/bin to PATH
```

Pass `--version v0.2.0`, `--prefix /usr/local/bin`, or `--repo yourname/run` to customize the install.

</details>

<details>
<summary><strong>Download the archive directly</strong></summary>

1. Grab the `tar.gz` (macOS/Linux) or `zip` (Windows) from the latest release.
2. Extract it and copy `run` / `run.exe` onto your `PATH`.
3. Optionally execute the bundled `install.sh` to handle the copy for you.

</details>

<details>
<summary><strong>Build from source</strong></summary>

```bash
cargo install run-kit
```

The project targets Rust 1.70+. Installing from crates.io gives you the same `run` binary that CI publishes; use `--force` when upgrading to a newer release.

</details>

##  How it works

`run` shells out to real toolchains under the hood. Each `LanguageEngine` implements a small trait that knows how to:

1. Detect whether the toolchain is available (e.g. `python3`, `go`, `rustc`).
2. Prepare a temporary workspace (compilation for compiled languages, transient scripts for interpreters).
3. Execute snippets, files, or stdin streams and surface stdout/stderr consistently.
4. Manage session state for the interactive REPL (persistent modules, stateful scripts, or regenerated translation units).

This architecture keeps the core lightweight while making it easy to add new runtimes or swap implementations.

## 🌍 Supported languages

`run` ships with 25+ batteries-included engines. Grouped by flavor:

| Category                  | Languages & aliases                                                                                                                                                                                    | Toolchain expectations                           |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------ |
| **Scripting & shells**    | Bash (`bash`), Python (`py`, `python`), Ruby (`rb`, `ruby`), PHP (`php`), Perl (`perl`), Lua (`lua`), R (`r`), Elixir (`ex`, `elixir`)                                                                 | Matching interpreter on `PATH`                   |
| **Web & typed scripting** | JavaScript (`js`, `node`), TypeScript (`ts`, `deno`), Dart (`dart`), Swift (`swift`), Kotlin (`kt`, `kotlin`)                                                                                          | `node`, `deno`, `dart`, `swift`, `kotlinc` + JRE |
| **Systems & compiled**    | C (`c`), C++ (`cpp`, `cxx`), Rust (`rs`, `rust`), Go (`go`), Zig (`zig`), Nim (`nim`), Haskell (`hs`, `haskell`), Crystal (`cr`, `crystal`), C# (`cs`, `csharp`), Java (`java`), Julia (`jl`, `julia`) | Respective compiler / toolchain                  |

Auto-detection heuristics consider file extensions and can fall back to the last language you used. Run `run :languages` inside the REPL to see the full list with availability checks.

##  Examples

Real programs live under the [`examples/`](examples) tree—each language has a `hello` and a `progress` scenario. The headers document expected output so you can diff your toolchain.

```bash
run examples/rust/hello.rs
run examples/typescript/progress.ts
run examples/python/counter.py
```

Use these as smoke tests or as a starting point for sharing snippets with your team.

##  REPL cheat sheet

| Command                    | Purpose                                      |
| -------------------------- | -------------------------------------------- |
| `:help`                    | List available meta commands                 |
| `:languages`               | Show detected engines and status             |
| `:lang <id>` or `:<alias>` | Switch the active language (`:py`, `:go`, …) |
| `:detect on/off/toggle`    | Control snippet language auto-detection      |
| `:load path/to/file`       | Execute a file inside the current session    |
| `:reset`                   | Clear the accumulated session state          |
| `:exit` / `:quit`          | Leave the REPL                               |

Language-specific tips (persistence model, auto-print behavior, etc.) are summarised in the built-in `:help` prompt.

##  Extending run

1. Add a new file in `src/engine/` implementing the `LanguageEngine` trait.
2. Register it inside `LanguageRegistry::bootstrap()` and provide aliases in `language::ALIASES`.
3. Add detection hints if the language benefits from extra heuristics.
4. Document usage with new examples and include integration tests.

Use the Python or Go engines as a template—they cover both scripting and compiled workflows.

## Testing & quality

```bash
cargo test
```

Tests will automatically skip engines if their toolchain is missing. For release parity, also run `cargo fmt`, `cargo clippy -- -D warnings`, and try a spot check via `run examples/python/counter.py`.



## 📄 License

Apache 2.0. See [LICENSE](LICENSE) for details.

---

Built with ❤️ in Rust. If `run` unblocks your workflow, star the repo and share it with other polyglot hackers.
