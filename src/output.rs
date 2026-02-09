use regex::Regex;

pub fn format_stderr(language: &str, stderr: &str, success: bool) -> String {
    if stderr.trim().is_empty() {
        return String::new();
    }

    let (scrubbed, _changed) = scrub_temp_paths(stderr);
    if success {
        return scrubbed;
    }

    format!("Error: {language} failed\n{scrubbed}")
}

fn scrub_temp_paths(stderr: &str) -> (String, bool) {
    let mut changed = false;
    let temp_dir = std::env::temp_dir();
    let temp = temp_dir.to_string_lossy();

    let mut output = stderr.to_string();
    if output.contains(temp.as_ref()) {
        output = output.replace(temp.as_ref(), "<temp>");
        changed = true;
    }

    if temp.as_ref().starts_with("/var/") {
        let private = format!("/private{temp}");
        if output.contains(&private) {
            output = output.replace(&private, "<temp>");
            changed = true;
        }
    }

    let re = Regex::new(r"<temp>/?(?:run-[^/]+|\.tmp[^/]+|run-[^/]+[^/]*)/").unwrap();
    if re.is_match(&output) {
        output = re.replace_all(&output, "<snippet>/").to_string();
        changed = true;
    }

    let re_noslash = Regex::new(r"<temp>\.tmp[^/]+/").unwrap();
    if re_noslash.is_match(&output) {
        output = re_noslash.replace_all(&output, "<snippet>/").to_string();
        changed = true;
    }

    (output, changed)
}
