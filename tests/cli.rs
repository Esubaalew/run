use std::io::Write;

use predicates::prelude::*;
use run::engine::LanguageEngine;
use tempfile::NamedTempFile;

fn python_available() -> bool {
    run::engine::PythonEngine::new().validate().is_ok()
}

fn bash_available() -> bool {
    run::engine::BashEngine::new().validate().is_ok()
}

fn javascript_available() -> bool {
    run::engine::JavascriptEngine::new().validate().is_ok()
}

fn ruby_available() -> bool {
    run::engine::RubyEngine::new().validate().is_ok()
}

fn typescript_available() -> bool {
    run::engine::TypeScriptEngine::new().validate().is_ok()
}

fn rust_available() -> bool {
    run::engine::RustEngine::new().validate().is_ok()
}

fn go_available() -> bool {
    run::engine::GoEngine::new().validate().is_ok()
}

fn csharp_available() -> bool {
    run::engine::CSharpEngine::new().validate().is_ok()
}

fn lua_available() -> bool {
    run::engine::LuaEngine::new().validate().is_ok()
}

fn c_available() -> bool {
    run::engine::CEngine::new().validate().is_ok()
}

fn cpp_available() -> bool {
    run::engine::CppEngine::new().validate().is_ok()
}

fn java_available() -> bool {
    run::engine::JavaEngine::new().validate().is_ok()
}

fn php_available() -> bool {
    run::engine::PhpEngine::new().validate().is_ok()
}

fn kotlin_available() -> bool {
    run::engine::KotlinEngine::new().validate().is_ok()
}

fn dart_available() -> bool {
    run::engine::DartEngine::new().validate().is_ok()
}

fn r_available() -> bool {
    run::engine::REngine::new().validate().is_ok()
}

fn swift_available() -> bool {
    run::engine::SwiftEngine::new().validate().is_ok()
}

fn perl_available() -> bool {
    run::engine::PerlEngine::new().validate().is_ok()
}

fn julia_available() -> bool {
    run::engine::JuliaEngine::new().validate().is_ok()
}

fn haskell_available() -> bool {
    run::engine::HaskellEngine::new().validate().is_ok()
}

fn elixir_available() -> bool {
    run::engine::ElixirEngine::new().validate().is_ok()
}

fn crystal_available() -> bool {
    run::engine::CrystalEngine::new().validate().is_ok()
}

fn zig_available() -> bool {
    run::engine::ZigEngine::new().validate().is_ok()
}

fn nim_available() -> bool {
    run::engine::NimEngine::new().validate().is_ok()
}

fn run_binary() -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin("run").expect("binary built")
}

#[test]
fn inline_python_execution() {
    if !python_available() {
        eprintln!("skipping python inline test: python interpreter not available");
        return;
    }

    run_binary()
        .args(["--lang", "python", "--code", "print('hello from inline')"])
        .assert()
        .success()
        .stdout(predicate::str::contains("hello from inline\n"));
}

#[test]
fn python_short_code_flag_reads_stdin() {
    if !python_available() {
        eprintln!("skipping python -c stdin test: python interpreter not available");
        return;
    }

    let code = r#"import sys, json; data = json.load(sys.stdin); print(f"{data['name']} is {data['age']} years old")"#;

    run_binary()
        .args(["py", "-c", code])
        .write_stdin("{\"name\":\"Ada\",\"age\":32}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("Ada is 32 years old\n"));
}

#[test]
fn inline_bash_execution() {
    if !bash_available() {
        eprintln!("skipping bash inline test: bash interpreter not available");
        return;
    }

    run_binary()
        .args(["--lang", "bash", "--code", "echo inline-bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-bash\n"));
}

#[test]
fn bash_file_execution() {
    if !bash_available() {
        eprintln!("skipping bash file test: bash interpreter not available");
        return;
    }

    let mut script = tempfile::Builder::new()
        .suffix(".go")
        .tempfile()
        .expect("temp file");
    writeln!(script, "echo from-file\nVALUE=$((40 + 2))\necho $VALUE").expect("write file");

    run_binary()
        .args(["bash", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from-file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn bash_stdin_execution() {
    if !bash_available() {
        eprintln!("skipping bash stdin test: bash interpreter not available");

        return;
    }

    run_binary()
        .args(["bash", "-"])
        .write_stdin("echo stdin-bash\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-bash\n"));
}

#[test]
fn inline_javascript_execution() {
    if !javascript_available() {
        eprintln!("skipping javascript inline test: node interpreter not available");
        return;
    }

    run_binary()
        .args(["--lang", "javascript", "--code", "console.log('inline-js')"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-js\n"));
}

#[test]
fn javascript_file_execution() {
    if !javascript_available() {
        eprintln!("skipping javascript file test: node interpreter not available");
        return;
    }

    let mut script = tempfile::Builder::new()
        .suffix(".go")
        .tempfile()
        .expect("temp file");
    writeln!(
        script,
        "console.log('from file');\nconst value = 21 * 2;\nconsole.log(value);"
    )
    .expect("write file");

    run_binary()
        .args(["javascript", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn javascript_stdin_execution() {
    if !javascript_available() {
        eprintln!("skipping javascript stdin test: node interpreter not available");
        return;
    }

    run_binary()
        .args(["javascript", "-"])
        .write_stdin("console.log('stdin-js')\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-js\n"));
}

#[test]
fn inline_ruby_execution() {
    if !ruby_available() {
        eprintln!("skipping ruby inline test: ruby interpreter not available");
        return;
    }

    run_binary()
        .args(["--lang", "ruby", "--code", "puts 'inline-ruby'"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-ruby\n"));
}

#[test]
fn ruby_file_execution() {
    if !ruby_available() {
        eprintln!("skipping ruby file test: ruby interpreter not available");
        return;
    }

    let mut script = tempfile::Builder::new()
        .suffix(".go")
        .tempfile()
        .expect("temp file");
    writeln!(script, "puts 'from file'\nvalue = 21 * 2\nputs value").expect("write file");

    run_binary()
        .args(["ruby", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn ruby_stdin_execution() {
    if !ruby_available() {
        eprintln!("skipping ruby stdin test: ruby interpreter not available");
        return;
    }

    run_binary()
        .args(["ruby", "-"])
        .write_stdin("puts 'stdin-ruby'\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-ruby\n"));
}

#[test]
fn ruby_session_interactivity() {
    if !ruby_available() {
        eprintln!("skipping ruby session test: ruby interpreter not available");
        return;
    }

    let engine = run::engine::RubyEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping ruby session test: irb not available");
        return;
    }

    let mut session = engine.start_session().expect("start ruby session");
    let expr = session.eval("1 + 2").expect("evaluate ruby expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("value = 40 + 2")
        .expect("define ruby variable");
    let follow = session.eval("value").expect("use ruby state");
    assert!(follow.stdout.contains("42"));

    session
        .eval("nums = [1, 2, 3]")
        .expect("store ruby collection");
    session
        .eval("nums.each do |n|")
        .expect("begin ruby multiline block");
    session
        .eval("  puts \"Number: #{n}\"")
        .expect("supply ruby block body");
    let loop_out = session.eval("end").expect("close ruby block");
    assert!(loop_out.stdout.contains("Number: 1"));
    assert!(loop_out.stdout.contains("Number: 3"));

    session.shutdown().expect("shutdown ruby session");
}

#[test]
fn inline_typescript_execution() {
    if !typescript_available() {
        eprintln!("skipping typescript inline test: deno interpreter not available");
        return;
    }

    run_binary()
        .args(["--lang", "typescript", "--code", "console.log('inline-ts')"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-ts\n"));
}

#[test]
fn typescript_file_execution() {
    if !typescript_available() {
        eprintln!("skipping typescript file test: deno interpreter not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "console.log('from file');\nconst value: number = 21 * 2;\nconsole.log(value);"
    )
    .expect("write file");

    run_binary()
        .args(["typescript", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn typescript_stdin_execution() {
    if !typescript_available() {
        eprintln!("skipping typescript stdin test: deno interpreter not available");
        return;
    }

    run_binary()
        .args(["typescript", "-"])
        .write_stdin("console.log('stdin-ts')\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-ts\n"));
}

#[test]
fn typescript_session_interactivity() {
    if !typescript_available() {
        eprintln!("skipping typescript session test: deno interpreter not available");
        return;
    }

    let engine = run::engine::TypeScriptEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping typescript session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start typescript session");
    let expr = session
        .eval("1 + 2")
        .expect("evaluate typescript expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("const value = 40 + 2;")
        .expect("define typescript variable");
    let follow = session.eval("value").expect("use typescript state");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown typescript session");
}

#[test]
fn inline_rust_execution() {
    if !rust_available() {
        eprintln!("skipping rust inline test: rustc not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "rust",
            "--code",
            "fn main() { println!(\"inline-rust\"); }",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-rust\n"));
}

#[test]
fn rust_file_execution() {
    if !rust_available() {
        eprintln!("skipping rust file test: rustc not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "fn main() {{\n    println!(\"from file\");\n    let value = 21 * 2;\n    println!(\"{{}}\", value);\n}}"
    )
    .expect("write file");

    run_binary()
        .args(["rust", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn rust_stdin_execution() {
    if !rust_available() {
        eprintln!("skipping rust stdin test: rustc not available");
        return;
    }

    run_binary()
        .args(["rust", "-"])
        .write_stdin("fn main() { println!(\"stdin-rust\"); }\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-rust\n"));
}

#[test]
fn rust_session_interactivity() {
    if !rust_available() {
        eprintln!("skipping rust session test: rustc not available");
        return;
    }

    let engine = run::engine::RustEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping rust session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start rust session");
    let expr = session.eval("1 + 2").expect("evaluate rust expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("let value = 40 + 2;")
        .expect("define rust variable");
    let follow = session.eval("value").expect("use rust state");
    assert!(follow.stdout.contains("42"));

    let full_program = session
        .eval("fn main() {\n    println!(\"hello from rust full program\");\n}\n")
        .expect("run standalone rust main");
    assert!(full_program.stdout.contains("hello from rust full program"));

    let after = session
        .eval("println!(\"still interactive\");")
        .expect("continue rust session");
    assert!(after.stdout.contains("still interactive"));

    session.shutdown().expect("shutdown rust session");
}

#[test]
fn javascript_session_interactivity() {
    if !javascript_available() {
        eprintln!("skipping javascript session test: node interpreter not available");
        return;
    }

    let engine = run::engine::JavascriptEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping javascript session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start javascript session");
    let first = session
        .eval("1 + 2")
        .expect("evaluate javascript expression");
    assert!(first.stdout.contains("3"));

    let _ = session
        .eval("const answer = 41;")
        .expect("define javascript variable");
    let second = session.eval("answer + 1").expect("use javascript state");
    assert!(second.stdout.contains("42"));

    session.shutdown().expect("shutdown javascript session");
}

#[test]
fn bash_session_interactivity() {
    if !bash_available() {
        eprintln!("skipping bash session test: bash interpreter not available");
        return;
    }

    let engine = run::engine::BashEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping bash session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start bash session");
    let expr = session
        .eval("echo $((1 + 2))")
        .expect("evaluate bash expression");
    assert!(expr.stdout.contains("3"));

    session.eval("value=41").expect("define bash variable");
    let follow = session.eval("echo $((value + 1))").expect("use bash state");
    assert!(follow.stdout.contains("42"));

    let loop_out = session
        .eval("for i in {0..2}; do\n  echo \"i=$i\"\ndone")
        .expect("run bash loop");
    assert!(loop_out.stdout.contains("i=0"));
    assert!(loop_out.stdout.contains("i=2"));

    let reset = session.eval(":reset").expect("reset bash session");
    assert!(reset.stdout.is_empty());
    assert!(reset.stderr.is_empty());

    let post_reset = session
        .eval("echo \"${value:-missing}\"")
        .expect("query cleared bash state");
    assert!(post_reset.stdout.contains("missing"));

    let help = session.eval(":help").expect("bash help message");
    assert!(help.stdout.contains("Bash commands"));

    session.shutdown().expect("shutdown bash session");
}

#[test]
fn java_session_interactivity() {
    if !java_available() {
        eprintln!("skipping java session test: java toolchain not available");
        return;
    }

    let engine = run::engine::JavaEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping java session test: jshell not available");
        return;
    }

    let mut session = engine.start_session().expect("start java session");
    let expr = session.eval("1 + 2").expect("evaluate java expression");
    assert!(expr.stdout.contains("3"));

    let _ = session
        .eval("int value = 40 + 2;")
        .expect("define java variable");
    let follow = session.eval("value").expect("use java state");
    eprintln!("follow stdout = {:?}", follow.stdout);
    eprintln!("follow stderr = {:?}", follow.stderr);
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown java session");
}

#[test]
fn kotlin_session_interactivity() {
    if !kotlin_available() {
        eprintln!("skipping kotlin session test: kotlinc not available");
        return;
    }

    let engine = run::engine::KotlinEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping kotlin session test: kotlin repl not available");
        return;
    }

    let mut session = engine.start_session().expect("start kotlin session");
    let expr = session.eval("1 + 2").expect("evaluate kotlin expression");
    assert!(expr.stdout.contains("3"));

    let _ = session
        .eval("val answer = 41")
        .expect("define kotlin value");
    let follow = session.eval("answer + 1").expect("use kotlin state");
    assert!(follow.stdout.contains("42"));

    let _ = session
        .eval("var s = \"Kotlin\"")
        .expect("define kotlin var");
    let assign = session.eval("s += \" REPL\"").expect("augment kotlin var");
    assert!(assign.stdout.is_empty());
    let appended = session.eval("println(s)").expect("print kotlin var");
    assert!(appended.stdout.contains("Kotlin REPL"));

    let help = session.eval(":help").expect("kotlin :help");
    assert!(help.stdout.contains("Kotlin commands"));

    let reset = session.eval(":reset").expect("kotlin :reset");
    assert!(reset.stdout.is_empty());
    assert!(reset.stderr.is_empty());

    let undefined = session.eval("answer").expect("expression after reset");
    assert!(undefined.exit_code.is_some());
    assert!(
        undefined.stdout.contains("Unresolved reference")
            || undefined.stderr.contains("Unresolved reference")
    );

    session.shutdown().expect("shutdown kotlin session");
}

#[test]
fn csharp_session_interactivity() {
    if !csharp_available() {
        eprintln!("skipping csharp session test: dotnet cli not available");
        return;
    }

    let engine = run::engine::CSharpEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping csharp session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start csharp session");
    let expr = session.eval("1 + 2").expect("evaluate csharp expression");
    assert!(expr.stdout.contains("3"));

    let _ = session
        .eval("var answer = 41;")
        .expect("define csharp variable");
    let follow = session.eval("answer + 1").expect("use csharp state");
    assert!(follow.stdout.contains("42"));

    let help = session.eval(":help").expect("csharp :help");
    assert!(help.stdout.contains("C# commands"));

    let reset = session.eval(":reset").expect("csharp :reset");
    assert!(reset.stdout.is_empty());
    assert!(reset.stderr.is_empty());

    let undefined = session.eval("answer").expect("expression after reset");
    assert!(undefined.exit_code.is_some());
    assert!(
        undefined.stdout.contains("CS0103")
            || undefined.stdout.contains("CS0116")
            || undefined.stderr.contains("CS0103")
            || undefined.stderr.contains("CS0116")
    );

    session.shutdown().expect("shutdown csharp session");
}

#[test]
fn cpp_session_interactivity() {
    if !cpp_available() {
        eprintln!("skipping cpp session test: c++ compiler not available");
        return;
    }

    let engine = run::engine::CppEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping cpp session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start cpp session");
    let expr = session.eval("1 + 2").expect("evaluate cpp expression");
    assert!(expr.stdout.contains("3"));

    let _ = session
        .eval("int answer = 41;")
        .expect("define cpp variable");
    let follow = session.eval("answer + 1").expect("use cpp state");
    assert!(follow.stdout.contains("42"));

    let full_program = session
        .eval(
            "#include <iostream>\nint main() {\n    std::cout << \"Hello, C++ REPL!\\n\";\n    return 0;\n}",
        )
        .expect("run standalone cpp program");
    assert!(full_program.stdout.contains("Hello, C++ REPL!"));

    let for_loop = session
        .eval("for (int i = 1; i <= 3; ++i) {\n    std::cout << \"Number: \" << i << std::endl;\n}")
        .expect("execute cpp block statement");
    assert!(for_loop.stdout.contains("Number: 1"));
    assert!(for_loop.stdout.contains("Number: 3"));

    let help = session.eval(":help").expect("cpp :help");
    assert!(help.stdout.contains("C++ commands"));

    let reset = session.eval(":reset").expect("cpp :reset");
    assert!(reset.stdout.is_empty());
    assert!(reset.stderr.is_empty());

    let undefined = session.eval("answer").expect("expression after reset");
    assert!(undefined.exit_code.is_some());
    assert!(undefined.stdout.is_empty());

    session
        .eval("int answer = 5;")
        .expect("define cpp variable post reset");
    let after_reset = session.eval("answer").expect("use cpp state after reset");
    assert!(after_reset.stdout.contains("5"));

    session.shutdown().expect("shutdown cpp session");
}

#[test]
fn inline_go_execution() {
    if !go_available() {
        eprintln!("skipping go inline test: go toolchain not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "go",
            "--code",
            "package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"inline-go\") }",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-go\n"));
}

#[test]
fn go_file_execution() {
    if !go_available() {
        eprintln!("skipping go file test: go toolchain not available");
        return;
    }

    let mut script = tempfile::Builder::new()
        .prefix("run-go-test")
        .suffix(".go")
        .tempfile()
        .expect("temp file");
    writeln!(
        script,
        "package main\nimport \"fmt\"\nfunc main() {{\n    fmt.Println(\"from file\")\n    value := 21 * 2\n    fmt.Println(value)\n}}"
    )
    .expect("write file");

    run_binary()
        .args(["go", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn go_stdin_execution() {
    if !go_available() {
        eprintln!("skipping go stdin test: go toolchain not available");
        return;
    }

    run_binary()
        .args(["go", "-"])
        .write_stdin("package main\nimport \"fmt\"\nfunc main() { fmt.Println(\"stdin-go\") }\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-go\n"));
}

#[test]
fn go_session_interactivity() {
    if !go_available() {
        eprintln!("skipping go session test: go toolchain not available");
        return;
    }

    let engine = run::engine::GoEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping go session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start go session");

    let expr = session.eval("1 + 2").expect("evaluate go expression");
    assert!(expr.stdout.contains("3"));

    session.eval("value := 40 + 2").expect("define go variable");
    let follow = session.eval("value").expect("use go state");
    assert!(follow.stdout.contains("42"));

    session.eval("import \"math\"").expect("add go import");
    let sqrt = session
        .eval("math.Sqrt(49)")
        .expect("use go import in expression");
    assert!(sqrt.stdout.contains("7"));

    let main_run = session
        .eval("func main() { fmt.Println(\"hello from go full program\") }")
        .expect("run go standalone main");
    assert!(main_run.stdout.contains("hello from go full program"));

    let after = session
        .eval("fmt.Println(\"still interactive\")")
        .expect("continue go session");
    assert!(after.stdout.contains("still interactive"));

    session.shutdown().expect("shutdown go session");
}

#[test]
fn inline_csharp_execution() {
    if !csharp_available() {
        eprintln!("skipping csharp inline test: csc/dotnet not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "csharp",
            "--code",
            "using System; class Program { static void Main() { Console.WriteLine(\"inline-csharp\"); } }",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-csharp\n"));
}

#[test]
fn csharp_file_execution() {
    if !csharp_available() {
        eprintln!("skipping csharp file test: csc/dotnet not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "using System;\nclass Program {{\n    static void Main() {{\n        Console.WriteLine(\"from file\");\n        var value = 21 * 2;\n        Console.WriteLine(value);\n    }}\n}}"
    )
    .expect("write file");

    run_binary()
        .args(["csharp", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn csharp_stdin_execution() {
    if !csharp_available() {
        eprintln!("skipping csharp stdin test: csc/dotnet not available");
        return;
    }

    run_binary()
        .args(["csharp", "-"])
        .write_stdin(
            "using System; class Program { static void Main() { Console.WriteLine(\"stdin-csharp\"); } }\n",
        )
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-csharp\n"));
}

#[test]
fn python_file_execution() {
    if !python_available() {
        eprintln!("skipping python file test: python interpreter not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "print('from file execution')\nvalue = 21 * 2\nprint(value)"
    )
    .expect("write file");

    run_binary()
        .args(["py", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("from file execution\n").and(predicate::str::contains("42\n")),
        );
}

#[test]
fn python_stdin_execution() {
    if !python_available() {
        eprintln!("skipping python stdin test: python interpreter not available");
        return;
    }

    run_binary()
        .args(["py", "-"])
        .write_stdin("print('stream hello')\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stream hello\n"));
}

#[test]
fn python_session_interactivity() {
    if !python_available() {
        eprintln!("skipping python session test: python interpreter not available");
        return;
    }

    let engine = run::engine::PythonEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping python session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start python session");

    let expr = session.eval("1 + 2").expect("evaluate python expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("value = 40 + 2")
        .expect("define python variable");

    let follow = session.eval("value").expect("use python state");
    assert!(follow.stdout.contains("42"));

    let loop_out = session
        .eval("for i in range(3):\n    print(f'i={i}')")
        .expect("run python loop");
    assert!(loop_out.stdout.contains("i=0"));
    assert!(loop_out.stdout.contains("i=2"));

    let reset = session.eval(":reset").expect("reset python session");
    assert!(reset.stdout.is_empty());

    let post_reset = session.eval("value").expect("query cleared state");
    assert!(post_reset.stderr.contains("NameError"));

    let help = session.eval(":help").expect("python help message");
    assert!(help.stdout.contains(":reset"));

    session.shutdown().expect("shutdown python session");
}

#[test]
fn inline_lua_execution() {
    if !lua_available() {
        eprintln!("skipping lua inline test: lua interpreter not available");
        return;
    }

    run_binary()
        .args(["--lang", "lua", "--code", "print(\"inline-lua\")"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-lua\n"));
}

#[test]
fn lua_file_execution() {
    if !lua_available() {
        eprintln!("skipping lua file test: lua interpreter not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "print(\"from file\")\nlocal value = 21 * 2\nprint(value)"
    )
    .expect("write file");

    run_binary()
        .args(["lua", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn lua_stdin_execution() {
    if !lua_available() {
        eprintln!("skipping lua stdin test: lua interpreter not available");
        return;
    }

    run_binary()
        .args(["lua", "-"])
        .write_stdin("print(\"stdin-lua\")\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-lua\n"));
}

#[test]
fn lua_session_interactivity() {
    if !lua_available() {
        eprintln!("skipping lua session test: lua interpreter not available");
        return;
    }

    let engine = run::engine::LuaEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping lua session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start lua session");
    let expr = session.eval("= 1 + 2").expect("evaluate lua expression");
    assert!(expr.stdout.contains("3"));

    session.eval("value = 40 + 2").expect("define lua variable");
    let follow = session.eval("= value").expect("use lua state");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown lua session");
}

#[test]
fn inline_c_execution() {
    if !c_available() {
        eprintln!("skipping c inline test: c compiler not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "c",
            "--code",
            "#include <stdio.h>\nint main(void) {\n    printf(\"inline-c\\n\");\n    return 0;\n}\n",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-c\n"));
}

#[test]
fn inline_c_execution_with_literal_newlines() {
    if !c_available() {
        eprintln!("skipping c escaped inline test: c compiler not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "c",
            "--code",
            "#include <stdio.h>\\nint main(void){\\n    puts(\"escaped inline-c\");\\n    return 0;\\n}\\n",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("escaped inline-c\n"));
}

#[test]
fn c_file_execution() {
    if !c_available() {
        eprintln!("skipping c file test: c compiler not available");
        return;
    }

    let mut script = tempfile::Builder::new()
        .prefix("run-c-test")
        .suffix(".c")
        .tempfile()
        .expect("temp file");
    writeln!(
        script,
        "#include <stdio.h>\nint main(void) {{\n    printf(\"from file\\n\");\n    int value = 21 * 2;\n    printf(\"%d\\n\", value);\n    return 0;\n}}"
    )
    .expect("write file");

    run_binary()
        .args(["c", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn c_stdin_execution() {
    if !c_available() {
        eprintln!("skipping c stdin test: c compiler not available");
        return;
    }

    run_binary()
        .args(["c", "-"])
        .write_stdin("#include <stdio.h>\nint main(void) { printf(\"stdin-c\\n\"); return 0; }\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-c\n"));
}

#[test]
fn c_session_interactivity() {
    if !c_available() {
        eprintln!("skipping c session test: c compiler not available");
        return;
    }

    let engine = run::engine::CEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping c session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start c session");

    let expr = session.eval("1 + 2").expect("evaluate c expression");
    assert!(expr.stdout.contains("3"));

    session.eval("int x = 20;").expect("define c variable");
    let follow = session.eval("x").expect("use c state");
    assert!(follow.stdout.contains("20"));

    session.eval("#include <string.h>").expect("add c include");
    let strlen_out = session
        .eval("strlen(\"run\")")
        .expect("use included header in expression");
    assert!(strlen_out.stdout.contains("3"));

    let full_program = session
        .eval("int main(void) {\n    printf(\"Hello, C REPL!\\n\");\n    return 0;\n}")
        .expect("run full c program");
    assert!(full_program.stdout.contains("Hello, C REPL!"));

    let printf_stmt = session
        .eval("printf(\"2 + 2 = %d\\n\", 2 + 2);")
        .expect("execute c statement after standalone program");
    assert!(printf_stmt.stdout.contains("2 + 2 = 4"));

    let reuse_state = session
        .eval("printf(\"x = %d\\n\", x);")
        .expect("use existing c state after standalone program");
    assert!(reuse_state.stdout.contains("x = 20"));

    let reset_outcome = session.eval(":reset").expect(":reset succeeds");
    assert!(reset_outcome.stdout.is_empty());
    assert!(reset_outcome.stderr.is_empty());

    let undefined = session.eval("x").expect("expression after reset");
    assert!(undefined.exit_code.is_some());
    assert!(undefined.stdout.is_empty());

    session
        .eval("int x = 5;")
        .expect("define variable after reset");
    let after_reset = session.eval("x").expect("use new variable");
    assert!(after_reset.stdout.contains("5"));

    session.shutdown().expect("shutdown c session");
}

#[test]
fn inline_cpp_execution() {
    if !cpp_available() {
        eprintln!("skipping cpp inline test: c++ compiler not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "cpp",
            "--code",
            "#include <iostream>\nint main() { std::cout << \"inline-cpp\\n\"; return 0; }\n",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-cpp\n"));
}

#[test]
fn inline_cpp_execution_with_literal_newlines() {
    if !cpp_available() {
        eprintln!("skipping cpp escaped inline test: c++ compiler not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "cpp",
            "--code",
            "#include <iostream>\\nint main(){\\n    std::cout << \"escaped inline-cpp\\n\";\\n    return 0;\\n}\\n",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("escaped inline-cpp\n"));
}

#[test]
fn cpp_file_execution() {
    if !cpp_available() {
        eprintln!("skipping cpp file test: c++ compiler not available");
        return;
    }

    let mut script = tempfile::Builder::new()
        .prefix("run-cpp-test")
        .suffix(".cpp")
        .tempfile()
        .expect("temp file");
    writeln!(
        script,
        "#include <iostream>\nint main() {{\n    std::cout << \"from file\\n\";\n    int value = 21 * 2;\n    std::cout << value << std::endl;\n    return 0;\n}}"
    )
    .expect("write file");

    run_binary()
        .args(["cpp", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn cpp_stdin_execution() {
    if !cpp_available() {
        eprintln!("skipping cpp stdin test: c++ compiler not available");
        return;
    }

    run_binary()
        .args(["cpp", "-"])
        .write_stdin(
            "#include <iostream>\nint main() { std::cout << \"stdin-cpp\\n\"; return 0; }\n",
        )
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-cpp\n"));
}

#[test]
fn inline_java_execution() {
    if !java_available() {
        eprintln!("skipping java inline test: java toolchain not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "java",
            "--code",
            "System.out.println(\"inline-java\");",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-java\n"));
}

#[test]
fn java_file_execution() {
    if !java_available() {
        eprintln!("skipping java file test: java toolchain not available");
        return;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let file_path = dir.path().join("Main.java");
    std::fs::write(
        &file_path,
        "public class Main {\n    public static void main(String[] args) {\n        System.out.println(\"from file\");\n        int value = 21 * 2;\n        System.out.println(value);\n    }\n}\n",
    )
    .expect("write file");

    run_binary()
        .args(["java", file_path.to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn java_stdin_execution() {
    if !java_available() {
        eprintln!("skipping java stdin test: java toolchain not available");
        return;
    }

    run_binary()
        .args(["java", "-"])
        .write_stdin("System.out.println(\"stdin-java\");\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-java\n"));
}

#[test]
fn inline_php_execution() {
    if !php_available() {
        eprintln!("skipping php inline test: php CLI not available");
        return;
    }

    run_binary()
        .args(["--lang", "php", "--code", "echo \"inline-php\\n\";"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-php\n"));
}

#[test]
fn php_file_execution() {
    if !php_available() {
        eprintln!("skipping php file test: php CLI not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "<?php\necho \"from file\\n\";\n$value = 21 * 2;\necho $value . \"\\n\";"
    )
    .expect("write file");

    run_binary()
        .args(["php", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn php_stdin_execution() {
    if !php_available() {
        eprintln!("skipping php stdin test: php CLI not available");
        return;
    }

    run_binary()
        .args(["php", "-"])
        .write_stdin("echo \"stdin-php\\n\";\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-php\n"));
}

#[test]
fn php_session_interactivity() {
    if !php_available() {
        eprintln!("skipping php session test: php CLI not available");
        return;
    }

    let engine = run::engine::PhpEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping php session test: interactive shell not available");
        return;
    }

    let mut session = engine.start_session().expect("start php session");
    let expr = session
        .eval("echo 1 + 2;")
        .expect("evaluate php expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("$value = 40 + 2;")
        .expect("define php variable");
    let follow = session.eval("echo $value;").expect("use php state");
    assert!(follow.stdout.contains("42"));

    let multiline = session
        .eval("if (true) {\n    echo \"block\";\n}\n")
        .expect("run php multiline block");
    assert!(multiline.stdout.contains("block"));

    let tagged = session
        .eval("<?php\necho \"tagged\";\n?>\n")
        .expect("run php snippet with tags");
    assert!(tagged.stdout.contains("tagged"));

    session.shutdown().expect("shutdown php session");
}

#[test]
fn inline_kotlin_execution() {
    if !kotlin_available() {
        eprintln!("skipping kotlin inline test: kotlin toolchain not available");
        return;
    }

    run_binary()
        .args(["--lang", "kotlin", "--code", "println(\"inline-kotlin\")"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-kotlin\n"));
}

#[test]
fn kotlin_file_execution() {
    if !kotlin_available() {
        eprintln!("skipping kotlin file test: kotlin toolchain not available");
        return;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let file_path = dir.path().join("Main.kt");
    std::fs::write(
        &file_path,
        "fun main() {\n    println(\"from file\");\n    val value = 21 * 2;\n    println(value);\n}\n",
    )
    .expect("write file");

    run_binary()
        .args(["kotlin", file_path.to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn kotlin_stdin_execution() {
    if !kotlin_available() {
        eprintln!("skipping kotlin stdin test: kotlin toolchain not available");
        return;
    }

    run_binary()
        .args(["kotlin", "-"])
        .write_stdin("println(\"stdin-kotlin\")\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-kotlin\n"));
}

#[test]
fn inline_r_execution() {
    if !r_available() {
        eprintln!("skipping r inline test: Rscript not available");
        return;
    }

    run_binary()
        .args(["--lang", "r", "--code", r#"cat("inline-r\n")"#])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-r\n"));
}

#[test]
fn r_file_execution() {
    if !r_available() {
        eprintln!("skipping r file test: Rscript not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(script, r#"cat("from file\n")"#).expect("write file");
    writeln!(script, "value <- 21 * 2").expect("write file");
    writeln!(script, r#"cat(value, "\n", sep = "")"#).expect("write file");

    run_binary()
        .args(["r", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn r_stdin_execution() {
    if !r_available() {
        eprintln!("skipping r stdin test: Rscript not available");
        return;
    }

    run_binary()
        .args(["r", "-"])
        .write_stdin("cat('stdin-r\\n')\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-r\n"));
}

#[test]
fn r_session_interactivity() {
    if !r_available() {
        eprintln!("skipping r session test: Rscript not available");
        return;
    }

    let engine = run::engine::REngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping r session test: interactive support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start r session");
    let expr = session.eval("1 + 2").expect("evaluate r expression");
    assert!(expr.stdout.contains("[1] 3"));

    session.eval("value <- 40 + 2").expect("define r variable");
    let follow = session.eval("value").expect("use r state");
    assert!(follow.stdout.contains("[1] 42"));

    let multiline = session
        .eval("for (i in 1:3) {\n  cat(sprintf(\"Number: %d\\n\", i))\n}\n")
        .expect("run r loop");
    assert!(multiline.stdout.contains("Number: 1"));
    assert!(multiline.stdout.contains("Number: 3"));

    session.shutdown().expect("shutdown r session");
}

#[test]
fn inline_dart_execution() {
    if !dart_available() {
        eprintln!("skipping dart inline test: dart CLI not available");
        return;
    }

    run_binary()
        .args(["--lang", "dart", "--code", "print('inline-dart');"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-dart\n"));
}

#[test]
fn dart_file_execution() {
    if !dart_available() {
        eprintln!("skipping dart file test: dart CLI not available");
        return;
    }

    let dir = tempfile::tempdir().expect("temp dir");
    let file_path = dir.path().join("main.dart");
    std::fs::write(
        &file_path,
        "void main() {\n  print('from file');\n  var value = 21 * 2;\n  print(value);\n}\n",
    )
    .expect("write file");

    run_binary()
        .args(["dart", file_path.to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn dart_stdin_execution() {
    if !dart_available() {
        eprintln!("skipping dart stdin test: dart CLI not available");
        return;
    }

    run_binary()
        .args(["dart", "-"])
        .write_stdin("print('stdin-dart');\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-dart\n"));
}

#[test]
fn dart_session_interactivity() {
    if !dart_available() {
        eprintln!("skipping dart session test: dart CLI not available");
        return;
    }

    let engine = run::engine::DartEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping dart session test: interactive support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start dart session");
    let expr = session.eval("1 + 2").expect("evaluate dart expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("var value = 40 + 2;")
        .expect("define dart variable");
    let follow = session.eval("value").expect("use dart state");
    assert!(follow.stdout.contains("42"));

    let multiline = session
        .eval("for (var i = 0; i < 3; i++) {\n  print('Number: $i');\n}\n")
        .expect("run dart loop");
    assert!(multiline.stdout.contains("Number: 0"));
    assert!(multiline.stdout.contains("Number: 2"));

    session.shutdown().expect("shutdown dart session");
}

#[test]
fn inline_swift_execution() {
    if !swift_available() {
        eprintln!("skipping swift inline test: swift executable not available");
        return;
    }

    run_binary()
        .args(["--lang", "swift", "--code", "print(\"inline-swift\")"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-swift\n"));
}

#[test]
fn swift_file_execution() {
    if !swift_available() {
        eprintln!("skipping swift file test: swift executable not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "print(\"from file\")\nlet value = 21 * 2\nprint(value)"
    )
    .expect("write file");

    run_binary()
        .args(["swift", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn swift_session_interactivity() {
    if !swift_available() {
        eprintln!("skipping swift session test: swift executable not available");
        return;
    }

    let engine = run::engine::SwiftEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping swift session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start swift session");
    let expr = session.eval("1 + 2").expect("evaluate swift expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("var value = 40 + 2")
        .expect("define swift variable");
    let follow = session.eval("value").expect("use swift state");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown swift session");
}

#[test]
fn inline_perl_execution() {
    if !perl_available() {
        eprintln!("skipping perl inline test: perl executable not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "perl",
            "--code",
            "use strict; use warnings; use feature 'say'; say 'inline-perl';",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-perl\n"));
}

#[test]
fn perl_file_execution() {
    if !perl_available() {
        eprintln!("skipping perl file test: perl executable not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "use strict;\nuse warnings;\nuse feature 'say';\nsay 'from file';\nmy $value = 21 * 2;\nsay $value;"
    )
    .expect("write file");

    run_binary()
        .args(["perl", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn perl_session_interactivity() {
    if !perl_available() {
        eprintln!("skipping perl session test: perl executable not available");
        return;
    }

    let engine = run::engine::PerlEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping perl session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start perl session");
    let expr = session.eval("1 + 2").expect("evaluate perl expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("my $value = 40 + 2;")
        .expect("define perl variable");
    let follow = session.eval("$value").expect("use perl state");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown perl session");
}

#[test]
fn inline_julia_execution() {
    if !julia_available() {
        eprintln!("skipping julia inline test: julia executable not available");
        return;
    }

    run_binary()
        .args(["--lang", "julia", "--code", "println(\"inline-julia\")"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-julia\n"));
}

#[test]
fn julia_file_execution() {
    if !julia_available() {
        eprintln!("skipping julia file test: julia executable not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "println(\"from file\")\nvalue = 21 * 2\nprintln(value)"
    )
    .expect("write file");

    run_binary()
        .args(["julia", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn julia_session_interactivity() {
    if !julia_available() {
        eprintln!("skipping julia session test: julia executable not available");
        return;
    }

    let engine = run::engine::JuliaEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping julia session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start julia session");
    let expr = session.eval("1 + 2").expect("evaluate julia expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("value = 40 + 2")
        .expect("define julia variable");
    let follow = session.eval("value").expect("use julia state");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown julia session");
}

#[test]
fn inline_haskell_execution() {
    if !haskell_available() {
        eprintln!("skipping haskell inline test: runghc executable not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "haskell",
            "--code",
            "main = putStrLn \"inline-haskell\"",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-haskell\n"));
}

#[test]
fn haskell_session_interactivity() {
    if !haskell_available() {
        eprintln!("skipping haskell session test: runghc executable not available");
        return;
    }

    let engine = run::engine::HaskellEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping haskell session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start haskell session");
    let expr = session.eval("1 + 2").expect("evaluate haskell expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("let value = 40 + 2")
        .expect("define haskell binding");
    let follow = session.eval("value").expect("use haskell state");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown haskell session");
}

#[test]
fn inline_elixir_execution() {
    if !elixir_available() {
        eprintln!("skipping elixir inline test: elixir executable not available");
        return;
    }

    run_binary()
        .args(["--lang", "elixir", "--code", "IO.puts(\"inline-elixir\")"])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-elixir\n"));
}

#[test]
fn elixir_session_interactivity() {
    if !elixir_available() {
        eprintln!("skipping elixir session test: elixir executable not available");
        return;
    }

    let engine = run::engine::ElixirEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping elixir session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start elixir session");
    let expr = session.eval("1 + 2").expect("evaluate elixir expression");
    assert!(expr.stdout.contains("3"));
    assert!(expr.stderr.is_empty());

    let assignment = session
        .eval("value = 40 + 2")
        .expect("define elixir variable");
    assert!(assignment.stderr.is_empty());

    let follow = session.eval("value").expect("use elixir state");
    assert!(follow.stdout.contains("42"));
    assert!(follow.stderr.is_empty());

    session.shutdown().expect("shutdown elixir session");
}

#[test]
fn inline_crystal_execution() {
    if !crystal_available() {
        eprintln!("skipping crystal inline test: crystal executable not available");
        return;
    }

    run_binary()
        .args(["--lang", "crystal", "--code", r#"puts "inline-crystal""#])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-crystal\n"));
}

#[test]
fn crystal_file_execution() {
    if !crystal_available() {
        eprintln!("skipping crystal file test: crystal executable not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "puts \"from file\"\nvalue : Int32 = 21 * 2\nputs value"
    )
    .expect("write file");

    run_binary()
        .args(["crystal", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn crystal_stdin_execution() {
    if !crystal_available() {
        eprintln!("skipping crystal stdin test: crystal executable not available");
        return;
    }

    run_binary()
        .args(["crystal", "-"])
        .write_stdin("puts \"stdin-crystal\"\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-crystal\n"));
}

#[test]
fn crystal_session_interactivity() {
    if !crystal_available() {
        eprintln!("skipping crystal session test: crystal executable not available");
        return;
    }

    let engine = run::engine::CrystalEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping crystal session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start crystal session");
    let expr = session.eval("1 + 2").expect("evaluate crystal expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("value = 40 + 2")
        .expect("define crystal variable");
    let follow = session.eval("value").expect("use crystal variable");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown crystal session");
}

#[test]
fn inline_zig_execution() {
    if !zig_available() {
        eprintln!("skipping zig inline test: zig executable not available");
        return;
    }

    run_binary()
        .args([
            "--lang",
            "zig",
            "--code",
            r#"const std = @import("std");
pub fn main() void {
    std.debug.print("inline-zig\\n", .{});
}
"#,
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-zig\n"));
}

#[test]
fn zig_file_execution() {
    if !zig_available() {
        eprintln!("skipping zig file test: zig executable not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(
        script,
        "const std = @import(\"std\");\npub fn main() void {{\n    std.debug.print(\"from file\\n\", .{{}});\n    const value: i32 = 21 * 2;\n    std.debug.print(\"{{d}}\\n\", .{{value}});\n}}"
    )
    .expect("write file");

    run_binary()
        .args(["zig", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn zig_stdin_execution() {
    if !zig_available() {
        eprintln!("skipping zig stdin test: zig executable not available");
        return;
    }

    run_binary()
        .args(["zig", "-"])
        .write_stdin(
            "const std = @import(\"std\");\npub fn main() void { std.debug.print(\"stdin-zig\\n\", .{}); }\n",
        )
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-zig\n"));
}

#[test]
fn zig_session_interactivity() {
    if !zig_available() {
        eprintln!("skipping zig session test: zig executable not available");
        return;
    }

    let engine = run::engine::ZigEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping zig session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start zig session");
    let expr = session.eval("1 + 2").expect("evaluate zig expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("const value = 40 + 2;")
        .expect("define zig constant");
    let follow = session.eval("value").expect("use zig constant");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown zig session");
}

#[test]
fn zig_session_numeric_suffix_literals() {
    if !zig_available() {
        eprintln!("skipping zig suffix test: zig executable not available");
        return;
    }

    let engine = run::engine::ZigEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping zig suffix test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start zig session");

    let expr = session.eval("10u64").expect("evaluate u64 literal");
    assert!(expr.stdout.contains("10"));
    assert!(expr.stderr.is_empty());

    let assign = session
        .eval("const speed = 10.0f32;")
        .expect("store f32 literal");
    assert!(assign.stderr.is_empty());

    let use_value = session.eval("speed").expect("reuse stored literal");
    assert!(use_value.stdout.contains("10"));
    assert!(use_value.stderr.is_empty());

    let print = session
        .eval("std.debug.print(\"{d}\n\", .{10u32});")
        .expect("print suffixed literal");
    assert!(print.stdout.contains("10"));
    assert!(print.stderr.is_empty());

    session.shutdown().expect("shutdown zig session");
}

#[test]
fn inline_nim_execution() {
    if !nim_available() {
        eprintln!("skipping nim inline test: nim executable not available");
        return;
    }

    run_binary()
        .args(["--lang", "nim", "--code", "echo \"inline-nim\""])
        .assert()
        .success()
        .stdout(predicate::str::contains("inline-nim\n"));
}

#[test]
fn nim_file_execution() {
    if !nim_available() {
        eprintln!("skipping nim file test: nim executable not available");
        return;
    }

    let mut script = NamedTempFile::new().expect("temp file");
    writeln!(script, "echo \"from file\"\nlet value = 21 * 2\necho value").expect("write file");

    run_binary()
        .args(["nim", script.path().to_str().expect("path utf8")])
        .assert()
        .success()
        .stdout(predicate::str::contains("from file\n").and(predicate::str::contains("42\n")));
}

#[test]
fn nim_stdin_execution() {
    if !nim_available() {
        eprintln!("skipping nim stdin test: nim executable not available");
        return;
    }

    run_binary()
        .args(["nim", "-"])
        .write_stdin("echo \"stdin-nim\"\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("stdin-nim\n"));
}

#[test]
fn nim_session_interactivity() {
    if !nim_available() {
        eprintln!("skipping nim session test: nim executable not available");
        return;
    }

    let engine = run::engine::NimEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping nim session test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start nim session");
    let expr = session.eval("1 + 2").expect("evaluate nim expression");
    assert!(expr.stdout.contains("3"));

    session
        .eval("let value = 40 + 2")
        .expect("define nim binding");
    let follow = session.eval("value").expect("use nim binding");
    assert!(follow.stdout.contains("42"));

    session.shutdown().expect("shutdown nim session");
}

#[test]
fn nim_session_unused_variable_suppressed() {
    if !nim_available() {
        eprintln!("skipping nim unused-variable test: nim executable not available");
        return;
    }

    let engine = run::engine::NimEngine::new();
    if !engine.supports_sessions() {
        eprintln!("skipping nim unused-variable test: session support unavailable");
        return;
    }

    let mut session = engine.start_session().expect("start nim session");

    let decl = session
        .eval("var age: int = 25")
        .expect("declare nim variable");
    assert!(decl.stderr.is_empty(), "unexpected stderr: {}", decl.stderr);

    let check = session.eval("age").expect("use nim variable");
    assert!(check.stdout.contains("25"));
    assert!(check.stderr.is_empty());

    let steps = session
        .eval(
            "let steps = @[\n  \"bootstrapping runtime\",\n  \"compiling modules\",\n  \"serving traffic\"\n]",
        )
        .expect("declare nim sequence");
    assert!(
        steps.stderr.is_empty(),
        "unexpected stderr: {}",
        steps.stderr
    );

    let loop_out = session
        .eval("for idx, step in steps.pairs():\n  echo \"[\" & $(idx + 1) & \"] \" & step")
        .expect("iterate nim sequence");
    assert!(loop_out.stdout.contains("[1] bootstrapping runtime"));
    assert!(loop_out.stdout.contains("[3] serving traffic"));
    assert!(
        loop_out.stderr.is_empty(),
        "unexpected stderr: {}",
        loop_out.stderr
    );

    let expr = session.eval("5 + 5").expect("evaluate nim expression");
    assert!(expr.stdout.contains("10"));
    assert!(expr.stderr.is_empty());

    session.shutdown().expect("shutdown nim session");
}
