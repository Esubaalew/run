use std::collections::HashMap;

use once_cell::sync::Lazy;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LanguageSpec {
    original: String,
    canonical: String,
}

impl LanguageSpec {
    pub fn new(token: impl Into<String>) -> Self {
        let raw = token.into();
        let canonical = canonical_language_id(&raw);
        Self {
            original: raw,
            canonical,
        }
    }

    pub fn canonical_id(&self) -> &str {
        &self.canonical
    }

    pub fn original(&self) -> &str {
        &self.original
    }
}

impl std::fmt::Display for LanguageSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.original.eq_ignore_ascii_case(&self.canonical) {
            write!(f, "{}", self.canonical)
        } else {
            write!(f, "{} ({})", self.canonical, self.original)
        }
    }
}

static ALIASES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    let pairs: &[(&str, &str)] = &[
        ("python", "python"),
        ("py", "python"),
        ("py3", "python"),
        ("python3", "python"),
        ("rust", "rust"),
        ("rs", "rust"),
        ("go", "go"),
        ("golang", "go"),
        ("csharp", "csharp"),
        ("cs", "csharp"),
        ("c#", "csharp"),
        ("dotnet", "csharp"),
        ("dotnetcore", "csharp"),
        ("typescript", "typescript"),
        ("ts", "typescript"),
        ("ts-node", "typescript"),
        ("javascript", "javascript"),
        ("js", "javascript"),
        ("node", "javascript"),
        ("nodejs", "javascript"),
        ("ecmascript", "javascript"),
        ("groovy", "groovy"),
        ("grv", "groovy"),
        ("groovysh", "groovy"),
        ("deno", "typescript"),
        ("denojs", "typescript"),
        ("ruby", "ruby"),
        ("rb", "ruby"),
        ("irb", "ruby"),
        ("lua", "lua"),
        ("luajit", "lua"),
        ("bash", "bash"),
        ("sh", "bash"),
        ("shell", "bash"),
        ("zsh", "bash"),
        ("java", "java"),
        ("c", "c"),
        ("cpp", "cpp"),
        ("c++", "cpp"),
        ("php", "php"),
        ("php-cli", "php"),
        ("kotlin", "kotlin"),
        ("kt", "kotlin"),
        ("kts", "kotlin"),
        ("r", "r"),
        ("rscript", "r"),
        ("cran", "r"),
        ("dart", "dart"),
        ("dartlang", "dart"),
        ("flutter", "dart"),
        ("swift", "swift"),
        ("swiftlang", "swift"),
        ("perl", "perl"),
        ("pl", "perl"),
        ("julia", "julia"),
        ("jl", "julia"),
        ("haskell", "haskell"),
        ("hs", "haskell"),
        ("ghci", "haskell"),
        ("elixir", "elixir"),
        ("ex", "elixir"),
        ("exs", "elixir"),
        ("iex", "elixir"),
        ("zig", "zig"),
        ("ziglang", "zig"),
        ("crystal", "crystal"),
        ("cr", "crystal"),
        ("crystal-lang", "crystal"),
        ("nim", "nim"),
        ("nimlang", "nim"),
    ];
    pairs.iter().cloned().collect()
});

pub fn canonical_language_id(token: &str) -> String {
    language_alias_lookup(token)
        .unwrap_or_else(|| token.trim())
        .to_ascii_lowercase()
}

pub fn language_alias_lookup(token: &str) -> Option<&'static str> {
    let normalized = token.trim().to_ascii_lowercase();
    ALIASES.get(normalized.as_str()).copied()
}

pub fn is_language_token(token: &str) -> bool {
    language_alias_lookup(token).is_some()
}

pub fn known_canonical_languages() -> Vec<&'static str> {
    let mut unique: Vec<_> = ALIASES.values().copied().collect();
    unique.sort_unstable();
    unique.dedup();
    unique
}
