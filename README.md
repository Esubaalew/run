<h1 align="center">run</h1>

<p align="center">
	<strong>Polyglot command runner & smart REPL that lets you script, compile, and iterate in 25+ languages without touching another CLI.</strong>
</p>

<p align="center">
  <!-- Release -->
  <a href="https://github.com/Esubaalew/run/releases/latest">
    <img src="https://img.shields.io/github/v/release/Esubaalew/run?style=flat-square&color=orange&logo=github" alt="Latest release" />
  </a>

  <!-- Release status -->
  <img src="https://img.shields.io/badge/release-passing-brightgreen?style=flat-square" alt="Release passing" />

  <!-- Docs -->
  <a href="https://docs.rs/run-kit">
    <img src="https://img.shields.io/badge/docs-passing-brightgreen?style=flat-square&logo=rust" alt="Docs passing" />
  </a>

  <!-- Crates.io -->
  <a href="https://crates.io/crates/run-kit">
    <img src="https://img.shields.io/crates/v/run-kit.svg?style=flat-square&logo=rust&color=red" alt="crates.io" />
  </a>

  <!-- Downloads -->
  <a href="https://github.com/Esubaalew/run/releases">
    <img src="https://img.shields.io/github/downloads/Esubaalew/run/total?style=flat-square&color=blue" alt="Downloads" />
  </a>

  <!-- Stars -->
  <a href="https://github.com/Esubaalew/run/stargazers">
    <img src="https://img.shields.io/github/stars/Esubaalew/run?style=flat-square&color=yellow" alt="GitHub stars" />
  </a>

  <!-- Platforms -->
  <img src="https://img.shields.io/badge/platform-Linux%20%7C%20macOS%20%7C%20Windows-lightgrey?style=flat-square" alt="Platform support" />

  <!-- License -->
  <a href="LICENSE">
    <img src="https://img.shields.io/badge/license-Apache%202.0-blue?style=flat-square" alt="License" />
  </a>
</p>

<p align="center">
	<a href="https://run.esubalew.et/">Website</a>
	•
	<a href="https://run.esubalew.et/docs/overview">Docs Overview</a>
</p>

> Built in Rust for developers who live in multiple runtimes. `run` gives you a consistent CLI, persistent REPLs, and batteries-included examples for your favorite languages.

---

## Run 2.0 (Experimental)

Run 2.0 adds WASI 0.2 component support for cross-language composition, instant startup, and edge deployment.

```bash
run v2 --help
```

**Quick Links:**
- [Run 2.0 Examples](examples/v2/)
- [Migration Guide](MIGRATION.md)
- [Registry Server](registry-server/)

See [Run 2.0 Documentation](#run-20---wasi-component-runtime) below for details.

---

<details>
<summary><strong>Table of contents</strong></summary>

- [Website and Docs](#website-and-docs)
- [Overview](#overview---universal-multi-language-runner)
  - [What is run?](#what-is-run)
  - [Who is this for?](#who-is-this-for)
  - [Why was run created?](#why-was-run-created)
  - [Why Rust?](#why-rust)
- [Quickstart](#quickstart)
- [Installation](#installation)
- [How it works](#how-it-works)
- [Supported languages](#supported-languages)
  - [Complete Language Aliases Reference](#complete-language-aliases-reference)
- [Command Variations - Flexible Syntax](#command-variations---flexible-syntax)
- [Command-Line Flags Reference](#command-line-flags-reference)
- [When to Use --lang](#️-when-to-use---lang-important)
- [Main Function Flexibility](#main-function-flexibility)
- [Examples](#examples)
- [REPL](#repl)
  - [Interactive REPL - Line by Line or Paste All](#interactive-repl---line-by-line-or-paste-all)
  - [Variable Persistence & Language Switching](#variable-persistence--language-switching)
  - [REPL Commands](#repl-commands)
- [Stdin Piping Examples](#stdin-piping-examples)
- [Language-Specific Notes](#language-specific-notes)
- [Run 2.0 - WASI Component Runtime](#run-20---wasi-component-runtime)
- [License](#license)

</details>

---

# Website and Docs

The official website and full documentation are available here:

- Website: https://run.esubalew.et/
- Docs Overview: https://run.esubalew.et/docs/overview

Use these links to explore features, language guides, and detailed examples.

---

# Overview - Universal Multi-Language Runner

A powerful command-line tool for executing code in 25 programming languages

## What is run?

run is a universal multi-language runner and smart REPL (Read-Eval-Print Loop) written in Rust. It provides a unified interface for executing code across 25 programming languages without the hassle of managing multiple compilers, interpreters, or build tools.

Whether you're a beginner learning your first programming language or an experienced polyglot developer, run streamlines your workflow by providing consistent commands and behavior across all supported languages.

## Who is this for?

- **Beginners:** Learn programming without worrying about complex setup procedures. Just install run and start coding in any language.
- **Students:** Quickly test code snippets and experiment with different programming paradigms across multiple languages.
- **Developers:** Prototype ideas rapidly, test algorithms, and switch between languages seamlessly without context switching.
- **DevOps Engineers:** Write and test automation scripts in various languages from a single tool.
- **Educators:** Teach programming concepts across multiple languages with a consistent interface.

## Why was run created?

Traditional development workflows require installing and configuring separate tools for each programming language. This creates several problems:

- **Time-consuming setup:** Installing compilers, interpreters, package managers, and configuring environments for each language.
- **Inconsistent interfaces:** Each language has different commands and flags for compilation and execution.
- **Cognitive overhead:** Remembering different commands and workflows for each language.
- **Barrier to entry:** Beginners struggle with setup before writing their first line of code.

run solves these problems by providing a single, unified interface that handles all the complexity behind the scenes. You focus on writing code, and run takes care of the rest.

## Why Rust?

run is built with Rust for several compelling reasons:

- **Performance:** Rust's zero-cost abstractions and efficient memory management ensure run starts instantly and executes with minimal overhead.
- **Reliability:** Rust's strong type system and ownership model prevent common bugs like null pointer dereferences and data races, making run stable and crash-resistant.
- **Cross-platform:** Rust compiles to native code for Windows, macOS, and Linux, providing consistent behavior across all platforms.
- **Memory safety:** No garbage collector means predictable performance without unexpected pauses.
- **Modern tooling:** Cargo (Rust's package manager) makes building and distributing run straightforward.
- **Future-proof:** Rust's growing ecosystem and industry adoption ensure long-term maintainability.

---

## Quickstart

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
echo '{"name":"Ada"}' | run js --code "const data = JSON.parse(require('fs').readFileSync(0, 'utf8')); console.log(\`hi \${data.name}\`)"

# Pipe stdin into Python
echo "Hello from stdin" | run python --code "import sys; print(sys.stdin.read().strip().upper())"

# Pipe stdin into Go
echo "world" | run go --code 'import "fmt"; import "bufio"; import "os"; scanner := bufio.NewScanner(os.Stdin); scanner.Scan(); fmt.Printf("Hello, %s!\n", scanner.Text())'
```

---

## Installation

All release assets are published on the [GitHub Releases](https://github.com/Esubaalew/run/releases) page, including macOS builds for both Apple Silicon (arm64) and Intel (x86_64). Pick the method that fits your platform:

<details>
<summary><strong>Cargo (Rust)</strong></summary>

```bash
cargo install run-kit
```

> Installs the `run` binary from the [`run-kit`](https://crates.io/crates/run-kit) crate. Updating? Run `cargo install run-kit --force`.

```bash
# Or build from source
git clone https://github.com/Esubaalew/run.git
cd run
cargo install --path .
```

> This builds the run binary using your active Rust toolchain. The project targets Rust 1.70 or newer.

</details>

<details>
<summary><strong>Homebrew (macOS)</strong></summary>

```bash
brew install --formula https://github.com/Esubaalew/run/releases/latest/download/homebrew-run.rb
```

> This formula is published as a standalone file on each release; it isn't part of the default Homebrew taps.

</details>

<details>
<summary><strong>Debian / Ubuntu</strong></summary>

```bash
ARCH=${ARCH:-amd64}
DEB_FILE=$(curl -s https://api.github.com/repos/Esubaalew/run/releases/latest \
  | grep -oE "run_[0-9.]+_${ARCH}\\.deb" | head -n 1)
curl -LO "https://github.com/Esubaalew/run/releases/latest/download/${DEB_FILE}"
curl -LO "https://github.com/Esubaalew/run/releases/latest/download/${DEB_FILE}.sha256"
sha256sum --check "${DEB_FILE}.sha256"
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
./install.sh --add-path
```

Pass `--version v0.2.0`, `--prefix /usr/local/bin`, or `--repo yourname/run` to customize the install.

</details>

Verify installation:

```bash
run --version
```

---

## How it works

`run` shells out to real toolchains under the hood. Each `LanguageEngine` implements a small trait that knows how to:

1. Detect whether the toolchain is available (e.g. `python3`, `go`, `rustc`).
2. Prepare a temporary workspace (compilation for compiled languages, transient scripts for interpreters).
3. Execute snippets, files, or stdin streams and surface stdout/stderr consistently.
4. Manage session state for the interactive REPL (persistent modules, stateful scripts, or regenerated translation units).

This architecture keeps the core lightweight while making it easy to add new runtimes or swap implementations.

---

## Supported languages

run supports 25 programming languages out of the box:

| Category                  | Languages & aliases                                                                                                                                                                                                     | Toolchain expectations                  |
| ------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------- |
| **Scripting & shells**    | Bash (`bash`), Python (`py`, `python`), Ruby (`rb`, `ruby`), PHP (`php`), Perl (`perl`), Groovy (`groovy`, `grv`), Lua (`lua`), R (`r`), Elixir (`ex`, `elixir`)                                                        | Matching interpreter on `PATH`          |
| **Web & typed scripting** | JavaScript (`js`, `node`), TypeScript (`ts`, `deno`), Dart (`dart`), Kotlin (`kt`, `kotlin`)                                                                                                                            | `node`, `deno`, `dart`, `kotlinc` + JRE |
| **Systems & compiled**    | C (`c`), C++ (`cpp`, `cxx`), Rust (`rs`, `rust`), Go (`go`), Swift (`swift`), Zig (`zig`), Nim (`nim`), Haskell (`hs`, `haskell`), Crystal (`cr`, `crystal`), C# (`cs`, `csharp`), Java (`java`), Julia (`jl`, `julia`) | Respective compiler / toolchain         |

### Complete Language Aliases Reference

| **Alias** | **Description** | **Badge** |
|------------|----------------|------------|
| `python, py, py3, python3` | Python programming language | ![Python](https://img.shields.io/badge/Python-3776AB?logo=python&logoColor=white) |
| `javascript, js, node, nodejs` | JavaScript (Node.js runtime) | ![JavaScript](https://img.shields.io/badge/JavaScript-F7DF1E?logo=javascript&logoColor=black) |
| `typescript, ts, ts-node, deno` | TypeScript with type checking | ![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?logo=typescript&logoColor=white) |
| `rust, rs` | Rust systems programming language | ![Rust](https://img.shields.io/badge/Rust-000000?logo=rust&logoColor=white) |
| `go, golang` | Go programming language | ![Go](https://img.shields.io/badge/Go-00ADD8?logo=go&logoColor=white) |
| `c, gcc, clang` | C programming language | ![C](https://img.shields.io/badge/C-A8B9CC?logo=c&logoColor=black) |
| `cpp, c++, g++` | C++ programming language | ![C++](https://img.shields.io/badge/C++-00599C?logo=cplusplus&logoColor=white) |
| `java` | Java programming language | ![Java](https://img.shields.io/badge/Java-007396?logo=java&logoColor=white) |
| `csharp, cs, dotnet` | C# (.NET) | ![C#](https://img.shields.io/badge/C%23-512BD4?logo=dotnet&logoColor=white) |
| `ruby, rb, irb` | Ruby programming language | ![Ruby](https://img.shields.io/badge/Ruby-CC342D?logo=ruby&logoColor=white) |
| `bash, sh, shell, zsh` | Bash shell scripting | ![Bash](https://img.shields.io/badge/Bash-4EAA25?logo=gnubash&logoColor=white) |
| `lua, luajit` | Lua scripting language | ![Lua](https://img.shields.io/badge/Lua-2C2D72?logo=lua&logoColor=white) |
| `perl, pl` | Perl programming language | ![Perl](https://img.shields.io/badge/Perl-39457E?logo=perl&logoColor=white) |
| `groovy, grv, groovysh` | Groovy on the JVM | ![Groovy](https://img.shields.io/badge/Groovy-4298B8?logo=apachegroovy&logoColor=white) |
| `php, php-cli` | PHP scripting language | ![PHP](https://img.shields.io/badge/PHP-777BB4?logo=php&logoColor=white) |
| `haskell, hs, ghci` | Haskell functional language | ![Haskell](https://img.shields.io/badge/Haskell-5D4F85?logo=haskell&logoColor=white) |
| `elixir, ex, exs, iex` | Elixir functional language | ![Elixir](https://img.shields.io/badge/Elixir-4B275F?logo=elixir&logoColor=white) |
| `julia, jl` | Julia scientific computing | ![Julia](https://img.shields.io/badge/Julia-9558B2?logo=julia&logoColor=white) |
| `dart, dartlang, flutter` | Dart language (Flutter) | ![Dart](https://img.shields.io/badge/Dart-0175C2?logo=dart&logoColor=white) |
| `swift, swiftlang` | Swift programming language | ![Swift](https://img.shields.io/badge/Swift-FA7343?logo=swift&logoColor=white) |
| `kotlin, kt, kts` | Kotlin (JVM/Native) | ![Kotlin](https://img.shields.io/badge/Kotlin-7F52FF?logo=kotlin&logoColor=white) |
| `r, rscript, cran` | R statistical computing | ![R](https://img.shields.io/badge/R-276DC3?logo=r&logoColor=white) |
| `crystal, cr, crystal-lang` | Crystal language | ![Crystal](https://img.shields.io/badge/Crystal-000000?logo=crystal&logoColor=white) |
| `zig, ziglang` | Zig systems language | ![Zig](https://img.shields.io/badge/Zig-F7A41D?logo=zig&logoColor=black) |
| `nim, nimlang` | Nim programming language | ![Nim](https://img.shields.io/badge/Nim-FFE953?logo=nim&logoColor=black) |

---

## Command Variations - Flexible Syntax

run supports multiple command formats:

```bash
# Full syntax
run --lang rust --code "fn main() { println!(\"hello\"); }"

# Shorthand flags
run -l rust -c "fn main() { println!(\"hello\"); }"

# Language first, then code
run rust "fn main() { println!(\"hello\"); }"

# Auto-detect from file
run examples/rust/hello.rs
```

---

## Command-Line Flags Reference

```bash
--lang, -l          Specify the programming language
--code, -c          Provide code as a string

run -l python -c "print('hello')"
run --lang python --code "print('hello')"
```

---

## When to Use --lang (Important!)

Always use `--lang` when syntax is ambiguous:

```bash
# Ambiguous - may choose wrong language
run "print('hello')"

# Explicit - always correct
run --lang python "print('hello')"
```

---

## Main Function Flexibility

For compiled languages, run is smart about main functions:

```bash
$ run go
go>>> fmt.Println("Hello, world!")
Hello, world!

go>>> package main
import "fmt"
func main() { fmt.Println("Hello!") }
Hello!
```

---

## Examples

Real programs live under the [`examples/`](examples) tree:

```bash
run examples/rust/hello.rs
run examples/typescript/progress.ts
run examples/python/counter.py
```

---

## REPL

The REPL supports built-in commands:

| Command                    | Purpose                                      |
| -------------------------- | -------------------------------------------- |
| `:help`                    | List available meta commands                 |
| `:languages`               | Show detected engines and status             |
| `:lang <id>` or `:<alias>` | Switch the active language (`:py`, `:go`, …) |
| `:detect on/off/toggle`    | Control snippet language auto-detection      |
| `:load path/to/file`       | Execute a file inside the current session    |
| `:reset`                   | Clear the accumulated session state          |
| `:exit` / `:quit`          | Leave the REPL                               |

### Interactive REPL - Line by Line or Paste All

```bash
$ run python
python>>> def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

for i in range(10):
    print(f"F({i}) = {fibonacci(i)}")
```

### Variable Persistence & Language Switching

```
$ run go
go>>> x := 10
go>>> x
10

go>>> :py
switched to python

python>>> y = 10
python>>> print(y)
10
```

---

## Stdin Piping Examples

```bash
# Node.js (JSON Processing)
echo '{"name":"Ada"}' | run js --code "const data = JSON.parse(require('fs').readFileSync(0, 'utf8')); console.log(\`hi \${data.name}\`)"

# Python (Uppercase)
echo "Hello" | run python --code "import sys; print(sys.stdin.read().strip().upper())"

# Go (Greeting)
echo "world" | run go --code 'import "fmt"; import "bufio"; import "os"; scanner := bufio.NewScanner(os.Stdin); scanner.Scan(); fmt.Printf("Hello, %s!\n", scanner.Text())'
```

---

## Language-Specific Notes

For detailed usage and best practices for each language, visit the [documentation](https://run.esubalew.et/docs/overview).

---

# Run 2.0 - WASI Component Runtime

Run 2.0 is an **experimental** extension that adds WASI 0.2 component support. It is opt-in and does not replace Run 1.0.

## What Run 2.0 Adds

- **Cross-language composition:** Rust, Python, Go, JS components calling each other via WIT interfaces
- **Instant startup:** <10ms cold start (vs Docker's 5-10 seconds)
- **Hermetic builds:** Reproducible builds with toolchain lockfiles
- **Edge deployment:** Deploy to Cloudflare Workers, AWS Lambda, Vercel

## Quick Start

```bash
# Install with v2 support
cargo install run-kit --features v2

# See v2 commands
run v2 --help

# Initialize a project
run v2 init my-app
cd my-app

# Build and run
run v2 build
run v2 dev
```

## Run 2.0 Commands

| Command | Description |
|---------|-------------|
| `run v2 init` | Initialize a new project |
| `run v2 build` | Build WASI components |
| `run v2 dev` | Development server with hot reload |
| `run v2 test` | Run component tests |
| `run v2 deploy` | Deploy to edge/registry |
| `run v2 install` | Install dependencies |

## Configuration

`run.toml` defines your project:

```toml
[package]
name = "my-app"
version = "1.0.0"

[[component]]
name = "api"
source = "src/lib.rs"
language = "rust"
wit = "wit/api.wit"

[dev]
watch = ["src/**/*.rs"]
hot_reload = true
```

## Resources

- [Run 2.0 Examples](examples/v2/) - Working examples and templates
- [Migration Guide](MIGRATION.md) - Migrate from Docker to Run 2.0
- [Registry Server](registry-server/) - Self-hosted component registry

---

## License

Apache 2.0. See [LICENSE](LICENSE) for details.

---

Built with Rust. If `run` helps your workflow, star the repo and share it with other polyglot developers.
