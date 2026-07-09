use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::engine::LanguageRegistry;
use crate::language::builtin_alias_lookup;

#[derive(Debug, Default, Serialize, Deserialize)]
struct AliasFile {
    #[serde(default)]
    aliases: BTreeMap<String, String>,
}

/// Path to the user alias file (`~/.config/run-kit/aliases.toml` on most platforms).
///
/// Override with `RUN_ALIASES_FILE` (full path) or `XDG_CONFIG_HOME` (base config dir).
pub fn config_path() -> PathBuf {
    if let Ok(path) = std::env::var("RUN_ALIASES_FILE") {
        return PathBuf::from(path);
    }

    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(dirs::config_dir)
        .unwrap_or_else(std::env::temp_dir);

    base.join("run-kit").join("aliases.toml")
}

pub fn normalize_token(token: &str) -> String {
    token.trim().to_ascii_lowercase()
}

pub fn load_custom_aliases() -> BTreeMap<String, String> {
    let path = config_path();
    if !path.is_file() {
        return BTreeMap::new();
    }
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str::<AliasFile>(&content)
            .map(|file| file.aliases)
            .unwrap_or_default(),
        Err(_) => BTreeMap::new(),
    }
}

pub fn custom_alias_lookup(token: &str) -> Option<String> {
    let key = normalize_token(token);
    load_custom_aliases().get(&key).cloned()
}

pub fn save_custom_aliases(aliases: &BTreeMap<String, String>) -> Result<()> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create config dir {}", parent.display()))?;
    }
    let file = AliasFile {
        aliases: aliases.clone(),
    };
    let content = toml::to_string_pretty(&file).context("serialize aliases.toml")?;
    std::fs::write(&path, content).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}

fn validate_alias_name(alias: &str) -> Result<String> {
    let normalized = normalize_token(alias);
    if normalized.is_empty() {
        bail!("alias name cannot be empty");
    }
    if normalized.contains(char::is_whitespace) {
        bail!("alias name cannot contain whitespace");
    }
    if builtin_alias_lookup(&normalized).is_some() {
        bail!("alias '{normalized}' is reserved by a built-in alias; choose a different name");
    }
    Ok(normalized)
}

fn validate_language_target(language: &str, registry: &LanguageRegistry) -> Result<String> {
    let spec = crate::language::LanguageSpec::new(language.to_string());
    let Some(engine) = registry.resolve(&spec) else {
        bail!("unknown language '{language}'. Run `run doctor` to see supported languages.");
    };
    Ok(engine.id().to_string())
}

pub fn add_alias(alias: &str, language: &str, registry: &LanguageRegistry) -> Result<()> {
    let alias = validate_alias_name(alias)?;
    let language = validate_language_target(language, registry)?;

    let mut aliases = load_custom_aliases();
    aliases.insert(alias.clone(), language.clone());
    save_custom_aliases(&aliases)?;
    println!("Added alias '{alias}' -> '{language}'");
    Ok(())
}

pub fn remove_alias(alias: &str) -> Result<()> {
    let alias = normalize_token(alias);
    if alias.is_empty() {
        bail!("alias name cannot be empty");
    }

    let mut aliases = load_custom_aliases();
    if aliases.remove(&alias).is_none() {
        bail!("custom alias '{alias}' not found");
    }
    save_custom_aliases(&aliases)?;
    println!("Removed alias '{alias}'");
    Ok(())
}

pub fn list_custom_aliases() -> Vec<(String, String)> {
    load_custom_aliases().into_iter().collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn with_temp_config<F: FnOnce(&PathBuf)>(f: F) {
        let _guard = TEST_LOCK.lock().expect("alias test lock");
        let temp = tempfile::tempdir().expect("tempdir");
        let aliases_file = temp.path().join("aliases.toml");
        unsafe { std::env::set_var("RUN_ALIASES_FILE", &aliases_file) };
        f(&aliases_file);
        unsafe { std::env::remove_var("RUN_ALIASES_FILE") };
    }

    #[test]
    fn add_and_remove_round_trip() {
        with_temp_config(|path| {
            let registry = LanguageRegistry::bootstrap();
            add_alias("p", "python", &registry).expect("add");
            assert!(path.is_file());
            assert_eq!(custom_alias_lookup("p").as_deref(), Some("python"));

            remove_alias("p").expect("remove");
            assert!(custom_alias_lookup("p").is_none());
        });
    }

    #[test]
    fn rejects_builtin_alias_name() {
        with_temp_config(|_| {
            let registry = LanguageRegistry::bootstrap();
            let err = add_alias("py", "python", &registry).unwrap_err();
            assert!(err.to_string().contains("reserved"));
        });
    }
}
