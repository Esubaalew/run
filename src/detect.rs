use once_cell::sync::Lazy;
use regex::Regex;

pub fn detect_language_from_snippet(code: &str) -> Option<&'static str> {
    let trimmed = code.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    if PYTHON_SIGNATURE.is_match(trimmed) {
        return Some("python");
    }
    if RUST_SIGNATURE.is_match(trimmed) {
        return Some("rust");
    }
    if GO_SIGNATURE.is_match(trimmed) {
        return Some("go");
    }
    if C_SHARP_SIGNATURE.is_match(trimmed) {
        return Some("csharp");
    }
    if CPP_SIGNATURE.is_match(trimmed) {
        return Some("cpp");
    }
    if C_SIGNATURE.is_match(trimmed) {
        return Some("c");
    }
    if JAVA_SIGNATURE.is_match(trimmed) {
        return Some("java");
    }
    if GROOVY_SIGNATURE.is_match(trimmed) {
        return Some("groovy");
    }
    if TYPESCRIPT_SIGNATURE.is_match(trimmed) {
        return Some("typescript");
    }
    if JAVASCRIPT_SIGNATURE.is_match(trimmed) {
        return Some("javascript");
    }
    if RUBY_SIGNATURE.is_match(trimmed) {
        return Some("ruby");
    }
    if KOTLIN_SIGNATURE.is_match(trimmed) {
        return Some("kotlin");
    }
    if PHP_SIGNATURE.is_match(trimmed) {
        return Some("php");
    }
    if LUA_SIGNATURE.is_match(trimmed) {
        return Some("lua");
    }
    if BASH_SIGNATURE.is_match(trimmed) {
        return Some("bash");
    }
    if R_SIGNATURE.is_match(trimmed) {
        return Some("r");
    }
    if DART_SIGNATURE.is_match(trimmed) {
        return Some("dart");
    }
    if SWIFT_SIGNATURE.is_match(trimmed) {
        return Some("swift");
    }
    if PERL_SIGNATURE.is_match(trimmed) {
        return Some("perl");
    }
    if JULIA_SIGNATURE.is_match(trimmed) {
        return Some("julia");
    }
    if HASKELL_SIGNATURE.is_match(trimmed) {
        return Some("haskell");
    }
    if ELIXIR_SIGNATURE.is_match(trimmed) {
        return Some("elixir");
    }
    if CRYSTAL_SIGNATURE.is_match(trimmed) {
        return Some("crystal");
    }
    if ZIG_SIGNATURE.is_match(trimmed) {
        return Some("zig");
    }
    if NIM_SIGNATURE.is_match(trimmed) {
        return Some("nim");
    }

    None
}

static PYTHON_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(from\s+[\w\.]+\s+import|import\s+[\w\.]+|def\s+[A-Za-z_][\w]*\(|class\s+[A-Za-z_])",
    )
    .expect("valid python regex")
});

static RUST_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(fn\s+main\s*\(|use\s+[\w:]+::|#!\[[^\n]+\]|mod\s+[A-Za-z_])"#)
        .expect("valid rust regex")
});

static GO_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(package\s+main|func\s+main\s*\(|import\s+(?:\w+\s+)?"[^"]+")"#)
        .expect("valid go regex")
});

static C_SHARP_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(using\s+System|namespace\s+[A-Za-z_][\w\.]*\s*\{|class\s+[A-Za-z_][\w]*\s*\{|\[assembly:)"#)
        .expect("valid csharp regex")
});

static C_SIGNATURE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?m)^(#include\s+<|int\s+main\s*\()"#).expect("valid c regex"));

static CPP_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(?:#include\s+<[^>]+>|using\s+namespace\s+std;|std::|int\s+main\s*\()"#)
        .expect("valid cpp regex")
});

static JAVA_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(package\s+[\w\.]+;|import\s+java\.|public\s+class\s+|class\s+\w+\s*\{\s*\n\s*public\s+static\s+void\s+main)"#)
        .expect("valid java regex")
});

static GROOVY_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?m)^(?:@Grab|@Grapes|println\s|def\s+\w+\s*=|import\s+groovy\.|class\s+\w+\s*\{|package\s+[\w\.]+)"#,
    )
    .expect("valid groovy regex")
});

static TYPESCRIPT_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(import\s+\{|type\s+\w+\s*=|interface\s+\w+|class\s+\w+\s+implements)")
        .expect("valid ts regex")
});

static JAVASCRIPT_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(import\s+(?:\w+\s+from\s+)?["']|console\.log|function\s+\w+\s*\(|module\.exports)"#)
        .expect("valid js regex")
});

static RUBY_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(require\s+['"]|class\s+\w+|module\s+\w+|puts\s)"#)
        .expect("valid ruby regex")
});

static KOTLIN_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(package\s+[\w\.]+|import\s+|fun\s+main\s*\(|val\s+\w+\s*=)"#)
        .expect("valid kotlin regex")
});

static PHP_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(?:<\?php|echo\s+['"]|function\s+\w+\s*\()"#).expect("valid php regex")
});

static LUA_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)^(local\s+function|function\s+\w+|print\s*\(|--\s)"#)
        .expect("valid lua regex")
});

static BASH_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"(?m)(^#!\s*/(?:usr/)?bin/(?:env\s+)?(?:bash|sh)|^(?:echo|export|read)\s+|\$\([\w\s]+\))"#)
        .expect("valid bash regex")
});

static R_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(library\(|require\(|print\(|cat\(|[A-Za-z_][\w.]*\s*<-|#[^!]|plot\()")
        .expect("valid r regex")
});

static DART_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^(import\s+'dart:|void\s+main\s*\(|class\s+\w+\s*\{|@override)")
        .expect("valid dart regex")
});

static SWIFT_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(import\s+Foundation|func\s+main\s*\(|print\(|class\s+\w+\s*:|struct\s+\w+\s*\{)",
    )
    .expect("valid swift regex")
});

static PERL_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(?:#!\s*/(?:usr/)?bin/(?:env\s+)?perl|use\s+(?:strict|warnings|feature)\b|my\s+\$|our\s+\$|sub\s+\w|print\s|say\s)"
    )
        .expect("valid perl regex")
});

static JULIA_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(using\s+|import\s+|function\s+\w|println\(|struct\s+\w|mutable\s+struct\s+\w)",
    )
    .expect("valid julia regex")
});

static HASKELL_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(module\s+\w+\s+where|import\s+[A-Z][\w\.]*|main\s*::\s*IO\s*\(|main\s*=|data\s+\w+\s*=|type\s+\w+|class\s+\w+|^\s*let\s+\w+\s*=)",
    )
    .expect("valid haskell regex")
});

static ELIXIR_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(defmodule\s+[A-Z][\w\.]*|defp?\s+\w+\s*\(|IO\.puts|IO\.inspect|alias\s+[A-Z][\w\.]*|use\s+[A-Z][\w\.]*|require\s+[A-Z][\w\.]*)",
    )
    .expect("valid elixir regex")
});

static CRYSTAL_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(?:@[A-Z][\w]*(?:\([^)]*\))?|struct\s+\w+|enum\s+\w+|record\s+\w+|macro\s+\w+|def\s+\w+\s*(?:\([^)]*\))?\s*:\s*[A-Z])",
    )
    .expect("valid crystal regex")
});

static ZIG_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?m)^(const\s+\w+\s*=\s*@import\("std"\)|pub\s+fn\s+main\s*\(|fn\s+main\s*\(\)\s*!?void)"#,
    )
    .expect("valid zig regex")
});

static NIM_SIGNATURE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^(proc\s+\w+\s*\(|import\s+[\w/]+|echo\s+|let\s+\w+\s*=|var\s+\w+\s*:\s*|template\s+\w+\s*\()",
    )
    .expect("valid nim regex")
});
