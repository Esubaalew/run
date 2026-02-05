use run::engine::{GroovyEngine, LanguageEngine};

fn groovy_available() -> bool {
    GroovyEngine::new().validate().is_ok()
}

fn assert_contains(haystack: &str, needle: &str, label: &str) {
    assert!(
        haystack.contains(needle),
        "{label}: expected output to contain '{needle}', got:\n{haystack}"
    );
}

#[test]
fn groovy_repl_semantics_suite_selected_cases() {
    if !groovy_available() {
        eprintln!("skipping groovy semantics suite: groovy not available");
        return;
    }

    let engine = GroovyEngine::new();
    let mut session = engine.start_session().expect("start groovy session");

    let out = session.eval("1 + 1").unwrap();
    assert!(out.success(), "1+1 failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "2", "1+1");

    let out = session.eval(r#""hello".toUpperCase()"#).unwrap();
    assert!(out.success(), "toUpperCase failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "HELLO", "toUpperCase");

    let out = session.eval("true && false").unwrap();
    assert!(out.success(), "and failed:\n{}", out.stderr);
    assert_contains(&out.stdout.to_lowercase(), "false", "true&&false");

    let out = session.eval("x = 10").unwrap();
    assert!(out.success(), "x=10 failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "10", "x=10 prints 10");

    let out = session.eval("x = x + 5").unwrap();
    assert!(out.success(), "x=x+5 failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "15", "x=x+5 prints 15");

    let out = session.eval("x += 5").unwrap();
    assert!(out.success(), "x+=5 failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "20", "x+=5 prints 20");

    let out = session.eval("{ 1 + 2 }").unwrap();
    assert!(out.success(), "{{1+2}} failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "3", "{1+2} prints 3");

    let out = session
        .eval(
            r#"
{
    def a = 10
    a * 2
}
"#,
        )
        .unwrap();
    assert!(out.success(), "multi-line closure failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "20", "multi-line closure prints 20");

    let out = session.eval("if (true) 10 else 20").unwrap();
    assert!(out.success(), "if expr failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "10", "if(true) 10 else 20");

    let out = session.eval(r#"if ([]) "yes" else "no""#).unwrap();
    assert!(out.success(), "truthiness failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "no", "if([]) -> no");

    let out = session
        .eval(r#"def name = null; name ?: "anonymous""#)
        .unwrap();
    assert!(out.success(), "elvis failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "anonymous", "elvis");

    let out = session.eval("def user = null; user?.name").unwrap();
    assert!(out.success(), "safe nav failed:\n{}", out.stderr);
    assert_contains(&out.stdout.to_lowercase(), "null", "safe nav null");
}
