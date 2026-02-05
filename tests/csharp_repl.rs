use run::engine::{CSharpEngine, LanguageEngine};

fn csharp_available() -> bool {
    CSharpEngine::new().validate().is_ok()
}

#[test]
fn csharp_session_prints_method_call_expressions_with_or_without_semicolon() {
    if !csharp_available() {
        eprintln!("skipping csharp session test: dotnet not available");
        return;
    }

    let engine = CSharpEngine::new();
    let mut session = engine.start_session().expect("start csharp session");

    let out = session
        .eval(r#"string[] names = {"dood", "dude"};"#)
        .unwrap();
    assert!(
        out.success(),
        "declaration should succeed; stderr was:\n{}",
        out.stderr
    );

    let out = session.eval(r#"names.Contains("dood")"#).unwrap();
    assert!(
        out.success(),
        "method call should succeed; stderr was:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.contains("True"),
        "expected output to contain True; got stdout:\n{}",
        out.stdout
    );

    let out = session.eval(r#"names.Contains("dood");"#).unwrap();
    assert!(
        out.success(),
        "method call with semicolon should succeed; stderr was:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.contains("True"),
        "expected output to contain True; got stdout:\n{}",
        out.stdout
    );
}

#[test]
fn csharp_session_prints_arithmetic_expression_with_trailing_semicolon() {
    if !csharp_available() {
        eprintln!("skipping csharp session test: dotnet not available");
        return;
    }

    let engine = CSharpEngine::new();
    let mut session = engine.start_session().expect("start csharp session");

    let out = session.eval("int d = 10;").unwrap();
    assert!(
        out.success(),
        "setup should succeed; stderr:\n{}",
        out.stderr
    );

    let out = session.eval("d + 10;").unwrap();
    assert!(
        out.success(),
        "expression with semicolon should succeed; stderr was:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.contains("20"),
        "expected output to contain 20; got stdout:\n{}",
        out.stdout
    );
}

#[test]
fn csharp_session_prints_member_access_expression_with_or_without_semicolon() {
    if !csharp_available() {
        eprintln!("skipping csharp session test: dotnet not available");
        return;
    }

    let engine = CSharpEngine::new();
    let mut session = engine.start_session().expect("start csharp session");

    let out = session.eval(r#""Hello".Length"#).unwrap();
    assert!(
        out.success(),
        "member access expression should succeed; stderr was:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.contains('5'),
        "expected output to contain 5; got stdout:\n{}",
        out.stdout
    );

    let out = session.eval(r#""Hello".Length;"#).unwrap();
    assert!(
        out.success(),
        "member access expression with semicolon should succeed; stderr was:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.contains('5'),
        "expected output to contain 5; got stdout:\n{}",
        out.stdout
    );
}

#[test]
fn csharp_session_prints_ternary_expression() {
    if !csharp_available() {
        eprintln!("skipping csharp session test: dotnet not available");
        return;
    }

    let engine = CSharpEngine::new();
    let mut session = engine.start_session().expect("start csharp session");

    let out = session.eval("int n = 7;").unwrap();
    assert!(
        out.success(),
        "setup should succeed; stderr:\n{}",
        out.stderr
    );

    let out = session.eval(r#"n % 2 == 0 ? "even" : "odd""#).unwrap();
    assert!(
        out.success(),
        "ternary expression should succeed; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.contains("odd"),
        "expected output to contain odd; got stdout:\n{}",
        out.stdout
    );

    let out = session.eval(r#"n % 2 == 0 ? "even" : "odd";"#).unwrap();
    assert!(
        out.success(),
        "ternary expression with semicolon should succeed; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.contains("odd"),
        "expected output to contain odd; got stdout:\n{}",
        out.stdout
    );
}
