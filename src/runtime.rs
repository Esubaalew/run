use std::sync::{LazyLock, Mutex};

#[derive(Debug, Default, Clone, Copy)]
struct RuntimeSettings {
    timeout_secs: Option<u64>,
    timing: bool,
}

static SETTINGS: LazyLock<Mutex<RuntimeSettings>> =
    LazyLock::new(|| Mutex::new(RuntimeSettings::default()));

/// Apply defaults from project configuration without overriding explicit CLI settings.
pub fn apply_config_defaults(timeout_secs: Option<u64>, timing: Option<bool>) {
    if let Ok(mut settings) = SETTINGS.lock() {
        if settings.timeout_secs.is_none() {
            settings.timeout_secs = timeout_secs;
        }
        if let Some(timing) = timing {
            settings.timing |= timing;
        }
    }
}

/// Override the execution timeout from CLI arguments.
pub fn set_timeout(timeout_secs: Option<u64>) {
    if let Ok(mut settings) = SETTINGS.lock() {
        settings.timeout_secs = timeout_secs;
    }
}

/// Enable timing output from CLI arguments.
pub fn enable_timing() {
    if let Ok(mut settings) = SETTINGS.lock() {
        settings.timing = true;
    }
}

/// Return the configured timeout in seconds, where zero means unlimited.
pub fn timeout_secs() -> u64 {
    if let Ok(value) = std::env::var("RUN_TIMEOUT_SECS")
        && let Ok(parsed) = value.parse::<u64>()
    {
        return parsed;
    }

    SETTINGS
        .lock()
        .ok()
        .and_then(|settings| settings.timeout_secs)
        .unwrap_or(0)
}

/// Return whether execution timing should be shown.
pub fn timing_enabled() -> bool {
    if std::env::var("RUN_TIMING").is_ok_and(|v| v == "1" || v == "true") {
        return true;
    }

    SETTINGS.lock().is_ok_and(|settings| settings.timing)
}
