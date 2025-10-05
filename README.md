
<h1 align="center">run</h1>

<p align="center">
	<strong>Polyglot command runner & smart REPL that lets you script, compile, and iterate in 25+ languages without touching another CLI.</strong>
</p>

<p align="center">
	<a href="https://github.com/Esubaalew/run/actions/workflows/release.yml"><img src="https://github.com/Esubaalew/run/actions/workflows/release.yml/badge.svg" alt="Release pipeline" /></a>
	<a href="https://github.com/Esubaalew/run/releases/latest"><img src="https://img.shields.io/github/v/release/Esubaalew/run?display_name=tag&sort=semver" alt="Latest release" /></a>
	<a href="https://github.com/Esubaalew/run/releases"><img src="https://img.shields.io/github/downloads/Esubaalew/run/total.svg" alt="Downloads" /></a>
	<a href="https://crates.io/crates/run-kit"><img src="https://img.shields.io/crates/v/run-kit.svg?label=crates.io&logo=rust" alt="crates.io" /></a>
	<a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache%202.0-blue.svg" alt="License" /></a>
</p>

<p align="center">
	<a href="https://run.esubalew.et/">Website</a>
	•
	<a href="https://run.esubalew.et/docs/overview">Docs Overview</a>
</p>

> Built in Rust for developers who live in multiple runtimes. `run` gives you a consistent CLI, persistent REPLs, and batteries-included examples for your favorite languages.

---

<details>
<summary><strong>Table of contents</strong></summary>

- [Website and Docs](#website-and-docs)
- [Overview](#overview---universal-multi-language-runner)
  - [What is run?](#what-is-run)
  - [Who is this for?](#who-is-this-for)
  - [Why was run created?](#why-was-run-created)
  - [Why Rust?](#why-rust)
- [Highlights](#-highlights)
- [Quickstart](#-quickstart)
- [Installation](#-installation)
- [How it works](#-how-it-works)
- [Supported languages](#-supported-languages)
  - [Complete Language Aliases Reference](#complete-language-aliases-reference)
- [Command Variations - Flexible Syntax](#command-variations---flexible-syntax)
- [Command-Line Flags Reference](#command-line-flags-reference)
- [⚠️ When to Use --lang (Important!)](#️-when-to-use---lang-important)
- [Main Function Flexibility](#main-function-flexibility)
- [Examples](#-examples)
- [REPL](#-repl)
  - [Interactive REPL - Line by Line or Paste All](#interactive-repl---line-by-line-or-paste-all)
  - [Variable Persistence & Language Switching](#variable-persistence--language-switching)
  - [REPL Commands](#repl-commands)
- [Stdin Piping Examples](#stdin-piping-examples)
- [Language-Specific Notes](#language-specific-notes)
- [📄 License](#-license)

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

• Beginners: Learn programming without worrying about complex setup procedures. Just install run and start coding in any language.

• Students: Quickly test code snippets and experiment with different programming paradigms across multiple languages.

• Developers: Prototype ideas rapidly, test algorithms, and switch between languages seamlessly without context switching.

• DevOps Engineers: Write and test automation scripts in various languages from a single tool.

• Educators: Teach programming concepts across multiple languages with a consistent interface.

## Why was run created?

Traditional development workflows require installing and configuring separate tools for each programming language. This creates several problems:

• Time-consuming setup: Installing compilers, interpreters, package managers, and configuring environments for each language.

• Inconsistent interfaces: Each language has different commands and flags for compilation and execution.

• Cognitive overhead: Remembering different commands and workflows for each language.

• Barrier to entry: Beginners struggle with setup before writing their first line of code.

run solves these problems by providing a single, unified interface that handles all the complexity behind the scenes. You focus on writing code, and run takes care of the rest.

## Why Rust?

run is built with Rust for several compelling reasons:

• Performance: Rust's zero-cost abstractions and efficient memory management ensure run starts instantly and executes with minimal overhead.

• Reliability: Rust's strong type system and ownership model prevent common bugs like null pointer dereferences and data races, making run stable and crash-resistant.

• Cross-platform: Rust compiles to native code for Windows, macOS, and Linux, providing consistent behavior across all platforms.

• Memory safety: No garbage collector means predictable performance without unexpected pauses.

• Modern tooling: Cargo (Rust's package manager) makes building and distributing run straightforward.

• Future-proof: Rust's growing ecosystem and industry adoption ensure long-term maintainability.

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
echo '{"name":"Ada"}' | run js --code "const data = JSON.parse(require('fs').readFileSync(0, 'utf8')); console.log(`hi ${data.name}`)"

# Pipe stdin into Python
echo "Hello from stdin" | run python --code "import sys; print(sys.stdin.read().strip().upper())"

# Pipe stdin into Go
echo "world" | run go --code 'import "fmt"; import "bufio"; import "os"; scanner := bufio.NewScanner(os.Stdin); scanner.Scan(); fmt.Printf("Hello, %s!\n", scanner.Text())'
```

---

## Installation

All release assets are published on the [GitHub Releases](https://github.com/Esubaalew/run/releases) page, including macOS builds for both Apple Silicon (arm64) and Intel (x86_64). Pick the method that fits your platform:

Installing run is straightforward. Choose the method that works best for your system:

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

> This formula is published as a standalone file on each release; it isn’t part of the default Homebrew taps. Installing by name (`brew install homebrew-run`) will fail—always point Homebrew to the release URL above (or download the file and run `brew install ./homebrew-run.rb`).

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

Verify installation:

```bash
# Verify installation
run --version
```

Output:

```
run 0.2.0
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

`run` supports 25+ languages:

run supports 25 programming languages out of the box, covering a wide range of paradigms and use cases:

```
# Scripting Languages
Python, JavaScript, Ruby, Bash, Lua, Perl, PHP

# Compiled Languages
Rust, Go, C, C++, Java, C#, Swift, Kotlin, Crystal, Zig, Nim

# Typed & Functional Languages
TypeScript, Haskell, Elixir, Julia

# Specialized Languages
R (Statistical computing)
Dart (Mobile development)
```

| Category                  | Languages & aliases                                                                                                                                                                                    | Toolchain expectations                           |
| ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------ |
| **Scripting & shells**    | Bash (`bash`), Python (`py`, `python`), Ruby (`rb`, `ruby`), PHP (`php`), Perl (`perl`), Lua (`lua`), R (`r`), Elixir (`ex`, `elixir`)                                                                 | Matching interpreter on `PATH`                   |
| **Web & typed scripting** | JavaScript (`js`, `node`), TypeScript (`ts`, `deno`), Dart (`dart`), Kotlin (`kt`, `kotlin`)                                                                                                          | `node`, `deno`, `dart`, `kotlinc` + JRE          |
| **Systems & compiled**    | C (`c`), C++ (`cpp`, `cxx`), Rust (`rs`, `rust`), Go (`go`), Swift (`swift`), Zig (`zig`), Nim (`nim`), Haskell (`hs`, `haskell`), Crystal (`cr`, `crystal`), C# (`cs`, `csharp`), Java (`java`), Julia (`jl`, `julia`) | Respective compiler / toolchain                  |

### Categorization notes

The categories above are usage-based to match how you’ll likely run code with `run` rather than strict language taxonomies. Examples:

- Kotlin can target the JVM, Native, or JavaScript. If you’re using Kotlin/JS, it behaves closer to the “Web & typed scripting” workflow, while Kotlin/JVM fits “Systems & compiled” (with a JRE).
- Swift is listed under “Systems & compiled” because `swiftc` produces native binaries; however, you can still use it interactively via `run` for scripting-like workflows.
- TypeScript typically runs via Node or Deno at runtime (transpiled), which is why it appears under “Web & typed scripting.”

These groupings optimize for how commands are invoked and which toolchains `run` detects and orchestrates.

### Complete Language Aliases Reference

Every language in run has multiple aliases for convenience. Use whichever feels most natural to you:

| Alias | Description |
|  --  |  --  |
| `python, py, py3, python3` | Python programming language |
| `javascript, js, node, nodejs` | JavaScript (Node.js runtime) |
| `typescript, ts, ts-node, deno` | TypeScript with type checking |
| `rust, rs` | Rust systems programming language |
| `go, golang` | Go programming language |
| `c, gcc, clang` | C programming language |
| `cpp, c++, g++` | C++ programming language |
| `java` | Java programming language |
| `csharp, cs, dotnet` | C# (.NET) |
| `ruby, rb, irb` | Ruby programming language |
| `bash, sh, shell, zsh` | Bash shell scripting |
| `lua, luajit` | Lua scripting language |
| `perl, pl` | Perl programming language |
| `php, php-cli` | PHP scripting language |
| `haskell, hs, ghci` | Haskell functional language |
| `elixir, ex, exs, iex` | Elixir functional language |
| `julia, jl` | Julia scientific computing |
| `dart, dartlang, flutter` | Dart language (Flutter) |
| `swift, swiftlang` | Swift programming language |
| `kotlin, kt, kts` | Kotlin (JVM/Native) |
| `r, rscript, cran` | R statistical computing |
| `crystal, cr, crystal-lang` | Crystal language |
| `zig, ziglang` | Zig systems language |
| `nim, nimlang` | Nim programming language |
| `ocaml` | OCaml functional language |
| `clojure, clj` | Clojure Lisp dialect |

---

## Command Variations - Flexible Syntax

run supports multiple command formats to fit your workflow. You can be explicit with --lang or let run auto-detect the language:

1. Full syntax with --lang and --code

```bash
run --lang rust --code "fn main() { println!(\"hello from rust\"); }"
```

Output:

```
hello from rust
```

2. Shorthand flags (-l for --lang, -c for --code)

```bash
run -l rust -c "fn main() { println!(\"hello from rust\"); }"
```

3. Omit --code flag (auto-detected)

```bash
run --code "fn main() { println!(\"hello from rust\"); }"
```

Output:

```
hello from rust
```

4. Shorthand - just the code

```bash
run "fn main() { println!(\"hello from rust\"); }"
```

Output:

```
hello from rust
```

5. Language first, then code

```bash
run rust "fn main() { println!(\"hello from rust\"); }"
```

Output:

```
hello from rust
```

---

## Command-Line Flags Reference

run provides both long-form and short-form flags for convenience:

```bash
# Language specification
--lang, -l          Specify the programming language
run --lang python "print('hello')"
run -l python "print('hello')"

# Code input
--code, -c          Provide code as a string
run --code "print('hello')"
run -c "print('hello')"

# Combined usage
run -l python -c "print('hello')"
run --lang python --code "print('hello')"
```

---

## ⚠️ When to Use --lang (Important!)

While run can auto-detect languages, ambiguous syntax can cause confusion. For example, print('hello') looks similar in Python, Ruby, Lua, and other languages. Always use --lang for correctness when the syntax is ambiguous or when you need deterministic behavior.

```bash
# ❌ Ambiguous - may choose wrong language
run "print('hello')"
```

Output:

```
hello  # But which language was used?
```

```bash
# ✅ Explicit - always correct
run --lang python "print('hello')"
```

Output:

```
hello  # Guaranteed to use Python
```

RECOMMENDATION: Always use --lang for correctness when:

• The syntax is ambiguous across multiple languages

• You want to ensure the exact language is used

• You're writing scripts or automation that must be deterministic

---

## Main Function Flexibility

For compiled languages (Rust, Go, C, C++, Java, etc.), run is smart about main functions:

• Write complete programs with main functions

• Write code without main functions (run wraps it automatically)

• Both approaches work in REPL mode and inline execution

Go Example - With main function

```bash
$ run go
run universal REPL. Type :help for commands.

go>>> package main
import "fmt"
func main() {
    fmt.Println("Hello, world!")
}
Hello, world!
```

Go Example - Without main function

```
go>>> fmt.Println("Hello, world!")
Hello, world!
```

---

## Examples

Real programs live under the [`examples/`](examples) tree—each language has a `hello` and a `progress` scenario. The headers document expected output so you can diff your toolchain.

```bash
run examples/rust/hello.rs
run examples/typescript/progress.ts
run examples/python/counter.py
```

---

## REPL

Being inside REPL we can use the ff commands

The REPL supports several built-in commands for managing your session:

| Command                    | Purpose                                      |
| -------------------------- | -------------------------------------------- |
| `:help`                    | List available meta commands                 |
| `:languages`               | Show detected engines and status             |
| `:lang <id>` or `:<alias>` | Switch the active language (`:py`, `:go`, …) |
| `:detect on/off/toggle`    | Control snippet language auto-detection      |
| `:load path/to/file`       | Execute a file inside the current session    |
| `:reset`                   | Clear the accumulated session state          |
| `:exit` / `:quit`          | Leave the REPL                               |

| Alias | Description |
|  --  |  --  |
| `:help` | Show available REPL commands |
| `:quit or :q` | Exit the REPL |
| `:clear or :c` | Clear the screen |
| `:reset` | Reset the session (clear all variables) |
| `:lang <language>` | Switch to a different language |
| `:py, :js, :go, etc.` | Quick language switch shortcuts |

### Interactive REPL - Line by Line or Paste All

The REPL mode is incredibly flexible. You can:

• Type code line by line interactively

• Paste entire programs at once

• Mix both approaches in the same session

This works for ALL supported languages!

Python Example - Paste entire program

```bash
$ run python
python>>> def fibonacci(n):
    if n <= 1:
        return n
    return fibonacci(n-1) + fibonacci(n-2)

for i in range(10):
    print(f"F({i}) = {fibonacci(i)}")
F(0) = 0
F(1) = 1
F(2) = 1
F(3) = 2
F(4) = 3
F(5) = 5
F(6) = 8
F(7) = 13
F(8) = 21
F(9) = 34
```

Python Example - Line by line

```
python>>> x = 10
python>>> y = 20
python>>> print(x + y)
30
```

### Variable Persistence & Language Switching

Variables persist across REPL commands within the same session. You can also switch languages on the fly using the :lang command (e.g., :c, :py, :go):

In REPL mode, variables persist across commands within the same language session. You can also switch languages on the fly using :lang commands.

When you switch languages, variables from the previous language do NOT carry over (each language has its own isolated session).

Variable Persistence Example

```
$ run go
go>>> x := 10
go>>> x
10

go>>> :c
switched to c

c>>> int x = 10;
c>>> x
10
c>>> 10 + 10
20

c>>> :py
switched to python

python>>> y = 10
python>>> y
10
python>>> print(y)
10
python>>> z = 4
python>>> z is y
False
python>>> z == y
False
```

### Language Switching Commands

Switch between languages instantly in REPL mode using colon commands

### Built-in REPL Commands

```
:help              → Show help and available commands
:languages         → List all supported languages
:clear             → Clear the screen
:exit or :quit     → Exit the REPL
:lang <language>   → Switch to a different language
Ctrl+D             → Exit the REPL
```

---

## Stdin Piping Examples

`run` supports piping input from stdin to your code snippets across all languages. Here are more examples for different languages:

### Node.js (JSON Processing)

```bash
echo '{"name":"Ada"}' | run js --code "const data = JSON.parse(require('fs').readFileSync(0, 'utf8')); console.log(`hi ${data.name}`)"
```

Output:

```
hi Ada
```

### Python (Uppercase Conversion)

```bash
echo "Hello from stdin" | run python --code "import sys; print(sys.stdin.read().strip().upper())"
```

Output:

```
HELLO FROM STDIN
```

### Go (Greeting)

```bash
echo "world" | run go --code 'import "fmt"; import "bufio"; import "os"; scanner := bufio.NewScanner(os.Stdin); scanner.Scan(); fmt.Printf("Hello, %s!\n", scanner.Text())'
```

Output:

```
Hello, world!
```

### Ruby (Line Counting)

```bash
echo -e "line1\nline2\nline3" | run ruby --code "puts gets(nil).lines.count"
```

Output:

```
3
```

### Bash (Echo with Prefix)

```bash
echo "input text" | run bash --code 'read line; echo "Processed: $line"'
```

Output:

```
Processed: input text
```

---

## Language-Specific Notes

For detailed usage, quirks, and best practices for each language, visit the dedicated documentation:

- [Python](https://run.esubalew.et/): Tips for scripting, data processing, and REPL persistence.
- [JavaScript/Node.js](https://run.esubalew.et/): Async code, modules, and stdin handling.
- [Rust](https://run.esubalew.et/): Compilation flags, error handling, and workspace management.
- [Go](https://run.esubalew.et/): Package imports, build optimizations, and concurrency examples.
- [C/C++](https://run.esubalew.et/): Compiler selection, linking, and multi-file support.
- [Java](https://run.esubalew.et/): Classpath management, JVM args, and enterprise patterns.
- [TypeScript](https://run.esubalew.et/): Type checking, Deno vs Node, and transpilation.
- [And more...](https://run.esubalew.et/docs/overview) for all 25+ languages including Ruby, PHP, Haskell, Elixir, and specialized ones like R and Julia.

Each language doc covers:
- Toolchain requirements and detection
- REPL-specific features (e.g., persistent state)
- Common pitfalls and workarounds
- Advanced examples (e.g., file I/O, networking)

---

## License

Apache 2.0. See [LICENSE](LICENSE) for details.

---

Built with ❤️ in Rust. If `run` unblocks your workflow, star the repo and share it with other polyglot hackers.
