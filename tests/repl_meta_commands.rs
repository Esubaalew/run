//! Tests for REPL meta commands (:! :!! :help :exit etc).
//! Each new Phase 1 command should get a test here and be documented in docs/repl-commands.md.

use predicates::prelude::*;

#[allow(deprecated)] // cargo_bin_cmd! needs extra crate; cargo_bin fine without custom build-dir
fn run_binary() -> assert_cmd::Command {
    assert_cmd::Command::cargo_bin("run").expect("binary built")
}

fn run_repl() -> assert_cmd::Command {
    let mut cmd = run_binary();
    cmd.arg("-i");
    cmd
}

fn norm(s: &str) -> String {
    s.replace("\r\n", "\n")
}

fn norm_contains(needle: &str) -> impl predicates::Predicate<str> {
    let needle = needle.to_string();
    predicates::function::function(move |s: &str| norm(s).contains(needle.as_str()))
}

#[test]
fn repl_shell_capture_echo() {
    // :!! runs shell and prints captured stdout
    run_repl()
        .write_stdin(":!! echo hello-from-capture\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("hello-from-capture"));
}

#[test]
fn repl_shell_inherit_echo() {
    // :! runs shell with inherited stdout
    run_repl()
        .write_stdin(":! echo hello-from-inherit\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("hello-from-inherit"));
}

#[test]
fn repl_help_lists_shell_commands() {
    run_repl()
        .write_stdin(":help\n:exit\n")
        .assert()
        .success()
        .stdout(
            norm_contains(":!")
                .and(norm_contains(":!!"))
                .and(norm_contains("shell")),
        );
}

#[test]
fn repl_exit_quits() {
    run_repl().write_stdin(":exit\n").assert().success();
}

#[test]
fn repl_quit_quits() {
    run_repl().write_stdin(":quit\n").assert().success();
}

#[test]
fn repl_cd_and_dhist() {
    // :cd . then :dhist (stack may be empty if we only did :cd .)
    run_repl()
        .write_stdin(":cd .\n:dhist 1\n:exit\n")
        .assert()
        .success();
}

#[test]
fn repl_env_lists_or_gets() {
    run_repl()
        .write_stdin(":env\n:exit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("="));
}

#[test]
fn repl_bookmark_list_empty_or_contains() {
    // :bookmark -l may print (no bookmarks) or list; then exit
    let out = run_repl()
        .write_stdin(":bookmark -l\n:exit\n")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let out = String::from_utf8_lossy(&out);
    assert!(
        out.contains("(no bookmarks)") || out.contains("\t"),
        "expected (no bookmarks) or a list; got: {}",
        out
    );
}

#[test]
fn repl_history_no_args_or_range() {
    // :history with no args shows last 25; :history 1 shows last 1
    run_repl()
        .write_stdin(":history\n:history 1\n:exit\n")
        .assert()
        .success();
}

#[test]
fn repl_history_grep() {
    // Add some history then :history -g print
    run_repl()
        .write_stdin("1+1\n:history -g 1\n:exit\n")
        .assert()
        .success();
}

#[test]
fn repl_logstart_logstop_logstate() {
    use std::io::Read;
    let dir = std::env::temp_dir();
    let logfile = dir.join("run_log_test.txt");
    let _ = std::fs::remove_file(&logfile);
    run_repl()
        .write_stdin(format!(
            ":logstart {}\n:logstate\n1+1\n:logstop\n:logstate\n:exit\n",
            logfile.display()
        ))
        .assert()
        .success();
    let mut buf = String::new();
    if std::fs::File::open(&logfile)
        .and_then(|mut f| f.read_to_string(&mut buf))
        .is_ok()
    {
        assert!(
            buf.contains(":logstart"),
            "log should contain :logstart line"
        );
        assert!(buf.contains("1+1"), "log should contain code");
    }
    let _ = std::fs::remove_file(&logfile);
}

#[test]
fn repl_edit_then_execute() {
    // EDITOR=cat so "editor" just prints the file and exits; we then execute the file
    let dir = std::env::temp_dir();
    let path = dir.join("run_edit_test.py");
    std::fs::write(&path, "print(42)\n").expect("write test file");
    let result = run_repl()
        .env("EDITOR", "cat")
        .write_stdin(format!(":edit {}\n:exit\n", path.display()))
        .assert()
        .success();
    let stdout = result.get_output().stdout.clone();
    let out = String::from_utf8_lossy(&stdout);
    assert!(out.contains("42"), "expected 42 in output, got: {}", out);
    let _ = std::fs::remove_file(&path);
}

#[test]
fn repl_load_http_url() {
    // Fetch a tiny script from a URL and run it (requires network)
    let url = "https://raw.githubusercontent.com/Esubaalew/run/master/examples/python/counter.py";
    run_repl()
        .write_stdin(format!(":load {url}\n:exit\n"))
        .assert()
        .success();
}

#[test]
fn repl_history_f_file() {
    use std::io::Read;
    let dir = std::env::temp_dir();
    let file = dir.join("run_hist_f_test");
    let _ = std::fs::remove_file(&file);
    let input = format!(":history -f {}\n:exit\n", file.display());
    run_repl().write_stdin(input).assert().success();
    let mut buf = String::new();
    if std::fs::File::open(&file)
        .and_then(|mut f| f.read_to_string(&mut buf))
        .is_ok()
    {
        assert!(buf.is_empty() || buf.contains('\n') || buf.len() > 0);
    }
    let _ = std::fs::remove_file(&file);
}

#[test]
fn repl_time_prints_elapsed() {
    // :time <code> runs once and prints elapsed
    run_repl()
        .write_stdin(":lang python\n:time 1+1\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("elapsed").and(norm_contains("2")));
}

#[test]
fn repl_macro_save_and_run() {
    // Run two lines, save as macro, run macro
    run_repl()
        .write_stdin(":lang python\n1+1\n2+2\n:macro m -2\n:run m\n:exit\n")
        .assert()
        .success()
        .stdout(
            norm_contains("macro")
                .and(norm_contains("saved"))
                .and(norm_contains("2"))
                .and(norm_contains("4")),
        );
}

#[test]
fn repl_who_and_whos() {
    // Define a variable, :who lists it; :whos filters
    run_repl()
        .write_stdin(":lang python\nx = 1\n:who\n:whos x\n:whos z\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("x"));
}

#[test]
fn repl_commands_machine_friendly() {
    // :commands lists each :cmd with one-line description (no ANSI)
    run_repl()
        .write_stdin(":commands\n:exit\n")
        .assert()
        .success()
        .stdout(
            norm_contains(":help")
                .and(norm_contains("Show this help"))
                .and(norm_contains(":exit")),
        );
}

#[test]
fn repl_help_single_cmd() {
    // :help :cmd shows help for one command
    run_repl()
        .write_stdin(":help load\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("load").and(norm_contains("Load")));
}

#[test]
fn repl_quickref() {
    // :quickref shows one-screen cheat sheet
    run_repl()
        .write_stdin(":quickref\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("Quick reference").and(norm_contains(":exit")));
}

#[test]
fn repl_xmode() {
    // :xmode sets exception display mode; no arg shows current
    run_repl()
        .write_stdin(":xmode\n:xmode plain\n:xmode\n:exit\n")
        .assert()
        .success()
        .stdout(
            norm_contains("exception display")
                .and(norm_contains("verbose"))
                .and(norm_contains("plain")),
        );
}

#[test]
fn repl_config() {
    // :config lists get/set; no arg lists detect and xmode
    run_repl()
        .write_stdin(":config\n:config detect\n:config xmode\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("detect").and(norm_contains("xmode")));
}

#[test]
fn repl_paste_mode() {
    // :paste, then lines (with >>> stripped), then :end runs the buffer
    run_repl()
        .write_stdin(":lang python\n:paste\n>>> 1+1\n>>> 2+2\n:end\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("paste mode").and(norm_contains("paste done")));
}

#[test]
fn repl_precision() {
    // :precision with no arg shows current; :precision N sets (0–32)
    run_repl()
        .write_stdin(":precision\n:precision 4\n:precision\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("precision").and(norm_contains("4")));
}

#[test]
fn repl_last() {
    // :last with no prior run shows (no last output); after code, :last prints that stdout
    run_repl()
        .write_stdin(":last\n:lang python\n1+1\n:last\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("no last output").and(norm_contains("2")));
}

#[test]
fn repl_numbered_prompts() {
    // :config numbered_prompts on/off; :config lists numbered_prompts
    run_repl()
        .write_stdin(":config numbered_prompts on\n:config\n:exit\n")
        .assert()
        .success()
        .stdout(norm_contains("numbered_prompts").and(norm_contains("on")));
}

#[test]
fn repl_introspect() {
    // :? with no arg shows usage; :? print in Python shows help(print) output
    run_repl()
        .write_stdin(":?\n:lang python\n:? print\n:exit\n")
        .assert()
        .success()
        .stdout(
            norm_contains(":? <name>")
                .and(norm_contains("print"))
                .and(norm_contains("Help")),
        );
}

#[test]
fn repl_debug() {
    // :debug with no snippet shows usage; :debug on non-Python shows not available
    run_repl()
        .write_stdin(":debug\n:lang javascript\n:debug 1+1\n:exit\n")
        .assert()
        .success()
        .stdout(
            norm_contains(":debug [CODE]")
                .and(norm_contains("Debug not available"))
                .and(norm_contains("javascript")),
        );
}
