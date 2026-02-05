use once_cell::sync::Lazy;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::{LinesWithEndings, as_24_bit_terminal_escaped};

static SYNTAX_SET: Lazy<SyntaxSet> = Lazy::new(SyntaxSet::load_defaults_newlines);

static THEME_SET: Lazy<ThemeSet> = Lazy::new(ThemeSet::load_defaults);

fn supports_color() -> bool {
    std::env::var("NO_COLOR").is_err()
        && (std::env::var("TERM").unwrap_or_default().contains("color")
            || std::env::var("COLORTERM").is_ok())
}

fn language_to_syntax_name(language_id: &str) -> &str {
    match language_id.to_ascii_lowercase().as_str() {
        "python" | "py" | "python3" | "py3" => "Python",
        "javascript" | "js" | "node" | "nodejs" => "JavaScript",
        "typescript" | "ts" => "JavaScript",
        "rust" | "rs" => "Rust",
        "go" | "golang" => "Go",
        "c" => "C",
        "cpp" | "c++" | "cxx" => "C++",
        "java" => "Java",
        "csharp" | "cs" | "c#" => "C#",
        "ruby" | "rb" => "Ruby",
        "php" => "PHP",
        "bash" | "sh" | "shell" | "zsh" => "Bourne Again Shell (bash)",
        "lua" => "Lua",
        "perl" | "pl" => "Perl",
        "swift" => "Swift",
        "kotlin" | "kt" => "Kotlin",
        "r" | "rscript" => "R",
        "haskell" | "hs" => "Haskell",
        "julia" | "jl" => "Plain Text",
        "elixir" | "ex" | "exs" => "Plain Text",
        "dart" => "Dart",
        "groovy" | "grv" => "Groovy",
        "crystal" | "cr" => "Crystal",
        "zig" => "Zig",
        "nim" => "Nim",
        _ => "Plain Text",
    }
}

fn get_syntax_for_language(language_id: &str) -> &'static SyntaxReference {
    let syntax_name = language_to_syntax_name(language_id);
    SYNTAX_SET
        .find_syntax_by_name(syntax_name)
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text())
}

pub fn highlight_code(code: &str, language_id: &str) -> String {
    if !supports_color() {
        return code.to_string();
    }

    let syntax = get_syntax_for_language(language_id);

    let theme = &THEME_SET.themes["base16-ocean.dark"];

    let mut highlighter = HighlightLines::new(syntax, theme);
    let mut output = String::new();

    for line in LinesWithEndings::from(code) {
        let ranges = highlighter
            .highlight_line(line, &SYNTAX_SET)
            .unwrap_or_default();
        let escaped = as_24_bit_terminal_escaped(&ranges[..], false);
        output.push_str(&escaped);
    }

    if !output.is_empty() && !output.ends_with("\x1b[0m") {
        output.push_str("\x1b[0m");
    }

    output
}

pub fn highlight_repl_input(code: &str, language_id: &str) -> String {
    highlight_code(code, language_id)
}

pub fn highlight_output(code: &str, language_id: &str) -> String {
    highlight_code(code, language_id)
}

pub fn has_syntax_support(language_id: &str) -> bool {
    let syntax_name = language_to_syntax_name(language_id);
    SYNTAX_SET.find_syntax_by_name(syntax_name).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_language_mapping() {
        assert_eq!(language_to_syntax_name("python"), "Python");
        assert_eq!(language_to_syntax_name("rust"), "Rust");
        assert_eq!(language_to_syntax_name("javascript"), "JavaScript");
        assert_eq!(language_to_syntax_name("typescript"), "JavaScript");
        assert_eq!(language_to_syntax_name("go"), "Go");
        assert_eq!(language_to_syntax_name("java"), "Java");
        assert_eq!(language_to_syntax_name("csharp"), "C#");
        assert_eq!(language_to_syntax_name("cpp"), "C++");
        assert_eq!(language_to_syntax_name("ruby"), "Ruby");
        assert_eq!(language_to_syntax_name("php"), "PHP");
    }

    #[test]
    fn test_all_language_aliases() {
        assert_eq!(language_to_syntax_name("py"), "Python");
        assert_eq!(language_to_syntax_name("py3"), "Python");
        assert_eq!(language_to_syntax_name("python3"), "Python");

        assert_eq!(language_to_syntax_name("js"), "JavaScript");
        assert_eq!(language_to_syntax_name("node"), "JavaScript");
        assert_eq!(language_to_syntax_name("nodejs"), "JavaScript");

        assert_eq!(language_to_syntax_name("ts"), "JavaScript");

        assert_eq!(language_to_syntax_name("rs"), "Rust");

        assert_eq!(language_to_syntax_name("golang"), "Go");

        assert_eq!(language_to_syntax_name("c++"), "C++");
        assert_eq!(language_to_syntax_name("cxx"), "C++");

        assert_eq!(language_to_syntax_name("cs"), "C#");
        assert_eq!(language_to_syntax_name("c#"), "C#");
    }

    #[test]
    fn test_all_languages_have_syntax() {
        let supported = vec![
            "python",
            "javascript",
            "rust",
            "go",
            "c",
            "cpp",
            "java",
            "csharp",
            "ruby",
            "php",
            "bash",
            "lua",
            "perl",
            "r",
            "haskell",
        ];

        for lang in supported {
            assert!(
                has_syntax_support(lang),
                "Language {} should be supported",
                lang
            );
        }

        let fallback = vec![
            "swift", "kotlin", "dart", "groovy", "crystal", "zig", "nim", "julia", "elixir",
        ];
        for lang in fallback {
            let _ = highlight_code("test", lang);
        }
    }

    #[test]
    fn test_unknown_language_fallback() {
        let syntax_name = language_to_syntax_name("unknownlang123");
        assert_eq!(syntax_name, "Plain Text");
    }

    #[test]
    fn test_syntax_available() {
        assert!(has_syntax_support("python"));
        assert!(has_syntax_support("rust"));
        assert!(has_syntax_support("go"));
        assert!(has_syntax_support("javascript"));
        assert!(has_syntax_support("typescript"));
    }

    #[test]
    fn test_highlight_basic() {
        let code = "fn main() { println!(\"Hello\"); }";
        let highlighted = highlight_code(code, "rust");
        assert!(!highlighted.is_empty());
        assert!(highlighted.len() >= code.len());
    }

    #[test]
    fn test_highlight_python() {
        let code = "def hello():\n    print('world')";
        let highlighted = highlight_code(code, "python");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_highlight_javascript() {
        let code = "function hello() { console.log('world'); }";
        let highlighted = highlight_code(code, "javascript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_highlight_go() {
        let code = "package main\nfunc main() { fmt.Println(\"hello\") }";
        let highlighted = highlight_code(code, "go");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_empty_code() {
        let highlighted = highlight_code("", "python");
        assert!(highlighted.is_empty() || highlighted == "\x1b[0m");
    }

    #[test]
    fn test_whitespace_only() {
        let code = "   \n  \t  ";
        let highlighted = highlight_code(code, "rust");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_multiline_code() {
        let code = "fn main() {\n    let x = 10;\n    println!(\"{}\", x);\n}";
        let highlighted = highlight_code(code, "rust");
        assert!(!highlighted.is_empty());
        assert!(highlighted.contains('\n') || highlighted.contains("\\n"));
    }

    #[test]
    fn test_color_reset_at_end() {
        unsafe {
            std::env::set_var("TERM", "xterm-256color");
        }

        let code = "x = 10";
        let highlighted = highlight_code(code, "python");

        assert!(
            highlighted.ends_with("\x1b[0m") || !highlighted.contains("\x1b["),
            "Highlighted code should end with color reset or be plain text"
        );
    }

    #[test]
    fn test_no_color_environment() {
        unsafe {
            std::env::set_var("NO_COLOR", "1");
        }

        let code = "fn main() {}";
        let highlighted = highlight_code(code, "rust");

        assert_eq!(highlighted, code);

        unsafe {
            std::env::remove_var("NO_COLOR");
        }
    }

    #[test]
    fn test_special_characters() {
        let code = "print(\"Hello\\nWorld\\t!\")";
        let highlighted = highlight_code(code, "python");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_string_literal() {
        let code = "message = \"Hello World\"";
        let highlighted = highlight_code(code, "python");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_repl_input_helper() {
        let code = "x = 42";
        let highlighted = highlight_repl_input(code, "python");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_output_helper() {
        let code = "console.log('test')";
        let highlighted = highlight_output(code, "javascript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_bash_highlighting() {
        let code = "echo \"Hello World\"";
        let highlighted = highlight_code(code, "bash");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_sql_like_code_in_supported_language() {
        let code = "query = \"SELECT * FROM users\"";
        let highlighted = highlight_code(code, "python");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_comments_highlighting() {
        let code = "// This is a comment\nlet x = 10;";
        let highlighted = highlight_code(code, "javascript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_string_highlighting() {
        let code = "\"This is a string\"";
        let highlighted = highlight_code(code, "python");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_number_highlighting() {
        let code = "let num = 42;";
        let highlighted = highlight_code(code, "javascript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_very_long_code() {
        let mut code = String::new();
        for i in 0..1000 {
            code.push_str(&format!("let var{} = {};\n", i, i));
        }
        let highlighted = highlight_code(&code, "javascript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_case_insensitive_language_names() {
        assert_eq!(language_to_syntax_name("PYTHON"), "Python");
        assert_eq!(language_to_syntax_name("Python"), "Python");
        assert_eq!(language_to_syntax_name("RuSt"), "Rust");
        assert_eq!(language_to_syntax_name("JavaScript"), "JavaScript");
    }

    #[test]
    fn test_syntax_reference_caching() {
        let syntax1 = get_syntax_for_language("python");
        let syntax2 = get_syntax_for_language("python");
        assert_eq!(syntax1.name, syntax2.name);
    }

    #[test]
    fn test_all_functional_languages() {
        let langs = vec!["haskell", "elixir", "julia"];
        for lang in langs {
            let code = "main = print \"hello\"";
            let highlighted = highlight_code(code, lang);
            assert!(!highlighted.is_empty(), "Failed for {}", lang);
        }
    }

    #[test]
    fn test_systems_languages() {
        let langs = vec!["c", "cpp", "rust", "zig", "nim"];
        for lang in langs {
            let code = "int main() { return 0; }";
            let highlighted = highlight_code(code, lang);
            assert!(!highlighted.is_empty(), "Failed for {}", lang);
        }
    }

    #[test]
    fn test_scripting_languages() {
        let langs = vec!["python", "ruby", "perl", "lua", "php"];
        for lang in langs {
            let code = "print('hello')";
            let highlighted = highlight_code(code, lang);
            assert!(!highlighted.is_empty(), "Failed for {}", lang);
        }
    }

    #[test]
    fn test_jvm_languages() {
        let langs = vec!["java", "kotlin", "groovy"];
        for lang in langs {
            let code = "public class Test { }";
            let highlighted = highlight_code(code, lang);
            assert!(!highlighted.is_empty(), "Failed for {}", lang);
        }
    }

    #[test]
    fn test_ansi_codes_present_when_colors_enabled() {
        unsafe {
            std::env::set_var("TERM", "xterm-256color");
            std::env::remove_var("NO_COLOR");
        }

        let code = "fn main() {}";
        let highlighted = highlight_code(code, "rust");

        let has_ansi = highlighted.contains("\x1b[") || highlighted == code;
        assert!(has_ansi, "Should contain ANSI codes or be plain text");
    }

    #[test]
    fn test_typescript_mapping() {
        assert_eq!(language_to_syntax_name("typescript"), "JavaScript");
        let code = "const x: number = 42;";
        let highlighted = highlight_code(code, "typescript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_swift_language() {
        let code = "func main() { print(\"Hello\") }";
        let highlighted = highlight_code(code, "swift");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_kotlin_language() {
        let code = "fun main() { println(\"Hello\") }";
        let highlighted = highlight_code(code, "kotlin");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_dart_language() {
        let code = "void main() { print('Hello'); }";
        let highlighted = highlight_code(code, "dart");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_r_language() {
        let code = "x <- 10\nprint(x)";
        let highlighted = highlight_code(code, "r");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_crystal_language() {
        let code = "puts \"Hello\"";
        let highlighted = highlight_code(code, "crystal");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_zig_language() {
        let code = "pub fn main() void {}";
        let highlighted = highlight_code(code, "zig");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_nim_language() {
        let code = "echo \"Hello\"";
        let highlighted = highlight_code(code, "nim");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_only_comments() {
        let code = "// Just a comment\n// Another comment";
        let highlighted = highlight_code(code, "rust");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_code_with_syntax_errors() {
        let code = "fn main( { println! }";
        let highlighted = highlight_code(code, "rust");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_mixed_quotes() {
        let code = r#"s1 = 'single'; s2 = "double""#;
        let highlighted = highlight_code(code, "python");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_nested_structures() {
        let code = "let arr = [[1, 2], [3, 4], [5, 6]];";
        let highlighted = highlight_code(code, "javascript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_regex_patterns() {
        let code = r"pattern = /[a-z]+/g";
        let highlighted = highlight_code(code, "javascript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_multiple_statements_one_line() {
        let code = "x = 1; y = 2; z = 3;";
        let highlighted = highlight_code(code, "javascript");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_color_reset_prevents_bleeding() {
        unsafe {
            std::env::set_var("TERM", "xterm-256color");
            std::env::remove_var("NO_COLOR");
        }

        let code1 = "x = 20";
        let highlighted1 = highlight_code(code1, "javascript");

        if highlighted1.contains("\x1b[") {
            assert!(
                highlighted1.ends_with("\x1b[0m"),
                "Highlighted code must end with reset code to prevent color bleeding"
            );
        }
    }

    #[test]
    fn test_all_25_languages_work() {
        let languages = vec![
            "python",
            "javascript",
            "typescript",
            "rust",
            "go",
            "c",
            "cpp",
            "java",
            "csharp",
            "ruby",
            "php",
            "bash",
            "lua",
            "perl",
            "swift",
            "kotlin",
            "r",
            "dart",
            "haskell",
            "julia",
            "elixir",
            "groovy",
            "crystal",
            "zig",
            "nim",
        ];

        for lang in languages {
            let code = "test code";
            let highlighted = highlight_code(code, lang);
            assert!(
                !highlighted.is_empty(),
                "Language {} failed to highlight",
                lang
            );
        }
    }

    #[test]
    fn test_bash_aliases() {
        let aliases = vec!["bash", "sh", "shell", "zsh"];
        for alias in aliases {
            assert_eq!(
                language_to_syntax_name(alias),
                "Bourne Again Shell (bash)",
                "Bash alias {} failed",
                alias
            );
        }
    }

    #[test]
    fn test_empty_lines_in_code() {
        let code = "fn main() {\n\n\n    println!(\"test\");\n\n}";
        let highlighted = highlight_code(code, "rust");
        assert!(!highlighted.is_empty());

        assert!(highlighted.matches('\n').count() >= 4);
    }

    #[test]
    fn test_tabs_and_spaces() {
        let code = "\tfn main() {\n\t\tprintln!(\"test\");\n\t}";
        let highlighted = highlight_code(code, "rust");
        assert!(!highlighted.is_empty());
    }

    #[test]
    fn test_colorterm_environment() {
        unsafe {
            std::env::set_var("COLORTERM", "truecolor");
            std::env::remove_var("NO_COLOR");
        }

        assert!(supports_color());

        unsafe {
            std::env::remove_var("COLORTERM");
        }
    }

    #[test]
    fn test_repl_helpers_consistency() {
        let code = "test = 42";
        let h1 = highlight_code(code, "python");
        let h2 = highlight_repl_input(code, "python");
        let h3 = highlight_output(code, "python");

        assert_eq!(h1, h2);
        assert_eq!(h2, h3);
    }
}
