use std::env;
use std::process::Command;

use chrono::Utc;

fn main() {
	println!("cargo:rerun-if-changed=build.rs");
	println!("cargo:rerun-if-env-changed=GIT_DIR");
	println!("cargo:rerun-if-changed=.git/HEAD");
	println!("cargo:rerun-if-changed=.git/refs");

	if let Err(err) = set_env("RUN_GIT_SHA", git(&["rev-parse", "--short", "HEAD"])) {
		eprintln!("warning: {err}");
	}
	if let Err(err) = set_env(
		"RUN_GIT_DATE",
		git(&["show", "-s", "--format=%cI", "HEAD"]),
	) {
		eprintln!("warning: {err}");
	}

	let dirty_state = git(&["status", "--porcelain"])
		.map(|output| {
			if output.trim().is_empty() {
				"clean".to_string()
			} else {
				"dirty".to_string()
			}
		})
		.unwrap_or_else(|_| "unknown".to_string());
	println!("cargo:rustc-env=RUN_GIT_DIRTY={dirty_state}");

	let timestamp = Utc::now().to_rfc3339();
	println!("cargo:rustc-env=RUN_BUILD_TIMESTAMP={timestamp}");

	let build_target = env::var("TARGET").unwrap_or_else(|_| "unknown".into());
	println!("cargo:rustc-env=RUN_BUILD_TARGET={build_target}");

	let profile = env::var("PROFILE").unwrap_or_else(|_| "unknown".into());
	println!("cargo:rustc-env=RUN_BUILD_PROFILE={profile}");

	if let Ok(rustc_version) = Command::new(env::var("RUSTC").unwrap_or_else(|_| "rustc".into()))
		.arg("--version")
		.output()
	{
		if rustc_version.status.success() {
			if let Ok(text) = String::from_utf8(rustc_version.stdout) {
				println!("cargo:rustc-env=RUN_RUSTC_VERSION={}", text.trim());
			}
		}
	}
}

fn git(args: &[&str]) -> Result<String, String> {
	let output = Command::new("git")
		.args(args)
		.output()
		.map_err(|err| err.to_string())?;
	if !output.status.success() {
		return Err(format!(
			"git {:?} failed with status {:?}",
			args,
			output.status.code()
		));
	}
	let text = String::from_utf8(output.stdout).map_err(|err| err.to_string())?;
	Ok(text.trim().to_string())
}

fn set_env(key: &str, value: Result<String, String>) -> Result<(), String> {
	match value {
		Ok(value) => {
			println!("cargo:rustc-env={key}={value}");
			Ok(())
		}
		Err(err) => Err(err),
	}
}