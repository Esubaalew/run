use run::engine::{GroovyEngine, LanguageEngine, TypeScriptEngine};

fn typescript_available() -> bool {
    TypeScriptEngine::new().validate().is_ok()
}

fn groovy_available() -> bool {
    GroovyEngine::new().validate().is_ok()
}

#[test]
fn typescript_session_prints_method_call_expression_with_or_without_semicolon() {
    if !typescript_available() {
        eprintln!("skipping typescript session test: deno not available");
        return;
    }

    let engine = TypeScriptEngine::new();
    let mut session = engine.start_session().expect("start typescript session");

    let out = session.eval(r#"const names = ["dood", "dude"];"#).unwrap();
    assert!(
        out.success(),
        "setup should succeed; stderr:\n{}",
        out.stderr
    );

    let out = session.eval(r#"names.includes("dood")"#).unwrap();
    assert!(
        out.success(),
        "expression should succeed; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.to_lowercase().contains("true"),
        "expected output to contain true; got stdout:\n{}",
        out.stdout
    );

    let out = session.eval(r#"names.includes("dood");"#).unwrap();
    assert!(
        out.success(),
        "expression with semicolon should succeed; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.to_lowercase().contains("true"),
        "expected output to contain true; got stdout:\n{}",
        out.stdout
    );
}

#[test]
fn groovy_session_prints_method_call_expression_with_or_without_semicolon() {
    if !groovy_available() {
        eprintln!("skipping groovy session test: groovy not available");
        return;
    }

    let engine = GroovyEngine::new();
    let mut session = engine.start_session().expect("start groovy session");

    let out = session.eval(r#"def names = ["dood", "dude"]"#).unwrap();
    assert!(
        out.success(),
        "setup should succeed; stderr:\n{}",
        out.stderr
    );

    let out = session.eval(r#"names.contains("dood")"#).unwrap();
    assert!(
        out.success(),
        "expression should succeed; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.to_lowercase().contains("true"),
        "expected output to contain true; got stdout:\n{}",
        out.stdout
    );

    let out = session.eval(r#"names.contains("dood");"#).unwrap();
    assert!(
        out.success(),
        "expression with semicolon should succeed; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stdout.to_lowercase().contains("true"),
        "expected output to contain true; got stdout:\n{}",
        out.stdout
    );
}
