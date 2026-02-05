use run::engine::{CSharpEngine, LanguageEngine};

fn csharp_available() -> bool {
    CSharpEngine::new().validate().is_ok()
}

fn assert_contains(haystack: &str, needle: &str, label: &str) {
    assert!(
        haystack.contains(needle),
        "{label}: expected output to contain '{needle}', got:\n{haystack}"
    );
}

fn assert_no_output(stdout: &str, stderr: &str, label: &str) {
    assert!(
        stdout.trim().is_empty(),
        "{label}: expected no stdout, got:\n{stdout}"
    );
    assert!(
        stderr.trim().is_empty(),
        "{label}: expected no stderr, got:\n{stderr}"
    );
}

#[test]
fn csharp_repl_semantics_suite_core_cases() {
    if !csharp_available() {
        eprintln!("skipping csharp semantics suite: dotnet not available");
        return;
    }

    let engine = CSharpEngine::new();
    let mut session = engine.start_session().expect("start csharp session");

    let out = session.eval("1 + 1").unwrap();
    assert!(out.success(), "1+1 failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "2", "1+1");

    let out = session.eval("Math.Max(3, 9)").unwrap();
    assert!(out.success(), "Math.Max failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "9", "Math.Max");

    let out = session.eval(r#""Hello".Length"#).unwrap();
    assert!(out.success(), "\"Hello\".Length failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "5", "\"Hello\".Length");

    let out = session.eval("DateTime.Now.Year").unwrap();
    assert!(out.success(), "DateTime.Now.Year failed:\n{}", out.stderr);
    let year = out.stdout.trim().parse::<i32>().unwrap_or(0);
    assert!(
        (2000..=3000).contains(&year),
        "DateTime.Now.Year expected a plausible year, got stdout:\n{}",
        out.stdout
    );

    let out = session.eval("true && false").unwrap();
    assert!(out.success(), "true && false failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "False", "true&&false");

    let out = session.eval("(10 + 5) * 2").unwrap();
    assert!(out.success(), "(10+5)*2 failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "30", "(10+5)*2");

    let out = session.eval("int x = 10;").unwrap();
    assert!(out.success(), "int x=10 failed:\n{}", out.stderr);
    assert_no_output(&out.stdout, &out.stderr, "int x=10 no output");

    let out = session.eval("x").unwrap();
    assert!(out.success(), "x failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "10", "x prints 10");

    let out = session.eval("x + 5").unwrap();
    assert!(out.success(), "x+5 failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "15", "x+5 prints 15");

    let out = session.eval("x = 20;").unwrap();
    assert!(out.success(), "x=20 failed:\n{}", out.stderr);
    assert_no_output(&out.stdout, &out.stderr, "x=20 no output");

    let out = session.eval("x").unwrap();
    assert!(out.success(), "x after assign failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "20", "x prints 20");

    let out = session.eval("x = 30").unwrap();
    assert!(out.success(), "x=30 failed:\n{}", out.stderr);
    assert_no_output(&out.stdout, &out.stderr, "x=30 no output");

    let out = session.eval("x++").unwrap();
    assert!(out.success(), "x++ failed:\n{}", out.stderr);
    assert_no_output(&out.stdout, &out.stderr, "x++ no output");

    let out = session.eval("--x").unwrap();
    assert!(out.success(), "--x failed:\n{}", out.stderr);
    assert_no_output(&out.stdout, &out.stderr, "--x no output");

    let out = session.eval("x += 5").unwrap();
    assert!(out.success(), "x += 5 failed:\n{}", out.stderr);
    assert_no_output(&out.stdout, &out.stderr, "x+=5 no output");

    let out = session.eval("x").unwrap();
    assert!(out.success(), "x final failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "35", "x final prints 35");

    let out = session.eval("int n = 7;").unwrap();
    assert!(out.success(), "int n=7 failed:\n{}", out.stderr);
    assert_no_output(&out.stdout, &out.stderr, "int n=7 no output");

    let out = session.eval(r#"n % 2 == 0 ? "even" : "odd""#).unwrap();
    assert!(out.success(), "ternary failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "odd", "ternary odd");

    let out = session.eval("false ? 1 : 2").unwrap();
    assert!(out.success(), "false?1:2 failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "2", "false?1:2 prints 2");

    let out = session.eval("new object()").unwrap();
    assert!(out.success(), "new object() failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "System.Object", "new object()");

    let out = session.eval("new List<int> { 1, 2, 3 }.Count").unwrap();
    assert!(out.success(), "list count failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "3", "list count");

    let out = session.eval(r#"new { Name = "Dire", Age = 5 }"#).unwrap();
    assert!(out.success(), "anon object failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "Dire", "anon object Name");
    assert_contains(&out.stdout, "Age", "anon object Age");

    let out = session
        .eval("new[] { 1, 2, 3 }.Select(x => x * 2).ToList()")
        .unwrap();
    assert!(out.success(), "linq tolist failed:\n{}", out.stderr);
    // Pretty-printed enumerable should look like [2, 4, 6]
    assert_contains(&out.stdout, "[", "linq tolist bracket");
    assert_contains(&out.stdout, "2", "linq tolist 2");
    assert_contains(&out.stdout, "4", "linq tolist 4");
    assert_contains(&out.stdout, "6", "linq tolist 6");

    let out = session
        .eval("new[] { 1, 2, 3 }.Where(x => x > 1).Count()")
        .unwrap();
    assert!(out.success(), "linq count failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "2", "linq count 2");

    let out = session.eval("object o = 10;").unwrap();
    assert!(out.success(), "object o=10 failed:\n{}", out.stderr);
    assert_no_output(&out.stdout, &out.stderr, "object o=10 no output");

    let out = session.eval(r#"o is int v ? v * 2 : 0"#).unwrap();
    assert!(
        out.success(),
        "pattern match ternary failed:\n{}",
        out.stderr
    );
    assert_contains(&out.stdout, "20", "pattern match 20");

    let out = session.eval("null").unwrap();
    assert!(out.success(), "null failed:\n{}", out.stderr);
    assert_contains(&out.stdout.trim(), "null", "null prints null");

    let out = session.eval("default(int)").unwrap();
    assert!(out.success(), "default(int) failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "0", "default(int)=0");

    let out = session.eval("typeof(int)").unwrap();
    assert!(out.success(), "typeof(int) failed:\n{}", out.stderr);
    assert_contains(&out.stdout, "System.Int32", "typeof(int)");

    let out = session.eval("nameof(StringBuilder)").unwrap();
    assert!(
        out.success(),
        "nameof(StringBuilder) failed:\n{}",
        out.stderr
    );
    assert_contains(&out.stdout, "StringBuilder", "nameof(StringBuilder)");
}
