const PKG_VERSION: &str = env!("CARGO_PKG_VERSION");
const PKG_NAME: &str = env!("CARGO_PKG_NAME");
const PKG_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
const PKG_AUTHORS: &str = env!("CARGO_PKG_AUTHORS");
const PKG_HOMEPAGE: Option<&str> = option_env!("CARGO_PKG_HOMEPAGE");
const PKG_REPOSITORY: Option<&str> = option_env!("CARGO_PKG_REPOSITORY");
const PKG_LICENSE: Option<&str> = option_env!("CARGO_PKG_LICENSE");
const BUILD_TARGET: Option<&str> = option_env!("RUN_BUILD_TARGET");
const BUILD_PROFILE: Option<&str> = option_env!("RUN_BUILD_PROFILE");
const BUILD_TIMESTAMP: Option<&str> = option_env!("RUN_BUILD_TIMESTAMP");
const GIT_SHA: Option<&str> = option_env!("RUN_GIT_SHA");
const GIT_DIRTY: Option<&str> = option_env!("RUN_GIT_DIRTY");
const GIT_DATE: Option<&str> = option_env!("RUN_GIT_DATE");
const RUSTC_VERSION: Option<&str> = option_env!("RUN_RUSTC_VERSION");

pub fn describe() -> String {
    let mut lines = Vec::new();
    lines.push(format!("{PKG_NAME} {PKG_VERSION}"));
    lines.push(PKG_DESCRIPTION.to_string());

    let authors = PKG_AUTHORS
        .split(':')
        .filter(|part| !part.trim().is_empty())
        .map(str::trim)
        .collect::<Vec<_>>()
        .join(", ");
    if !authors.is_empty() {
        lines.push(format!("author: {authors}"));
    }

    if let Some(homepage) = PKG_HOMEPAGE {
        lines.push(format!("homepage: {homepage}"));
    }
    if let Some(repo) = PKG_REPOSITORY {
        lines.push(format!("repository: {repo}"));
    }
    if let Some(license) = PKG_LICENSE {
        lines.push(format!("license: {license}"));
    }

    lines.push(format!(
        "commit: {} ({}, dirty: {})",
        GIT_SHA.unwrap_or("unknown"),
        GIT_DATE.unwrap_or("unknown date"),
        GIT_DIRTY.unwrap_or("unknown"),
    ));
    lines.push(format!(
        "built: {} [{} for {}]",
        BUILD_TIMESTAMP.unwrap_or("unknown time"),
        BUILD_PROFILE.unwrap_or("unknown profile"),
        BUILD_TARGET.unwrap_or("unknown target"),
    ));
    lines.push(format!(
        "rustc: {}",
        RUSTC_VERSION.unwrap_or("unknown rustc")
    ));
    lines.join("\n")
}
