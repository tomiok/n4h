use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_n4h");
const SKILL: &str = include_str!("../skills/n4h/SKILL.md");

struct Fixture(TempDir);

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        for folder in ["home", "config", "project"] {
            fs::create_dir(dir.path().join(folder)).unwrap();
        }
        Self(dir)
    }
    fn path(&self, path: &str) -> std::path::PathBuf {
        self.0.path().join(path)
    }
    fn command(&self) -> Command {
        let mut command = Command::new(BIN);
        command
            .current_dir(self.path("project"))
            .env("HOME", self.path("home"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env_remove("N4H_TEST_FAILPOINT")
            .stdin(Stdio::null());
        command
    }
    fn run(&self, args: &[&str], exit: i32) -> Value {
        let out = self.command().args(args).output().unwrap();
        decode(out, exit)
    }
}

fn decode(out: Output, exit: i32) -> Value {
    assert_eq!(
        out.status.code(),
        Some(exit),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.stdout.len() <= 65536);
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn setup_installs_identical_portable_skills_for_all_harnesses_without_project_state() {
    let f = Fixture::new();
    let result = f.run(
        &[
            "setup",
            "--harness",
            "claude-code,codex,opencode",
            "--scope",
            "user",
            "--yes",
        ],
        0,
    );
    assert_eq!(result["project_initialized"], false);
    for file in [
        "home/.claude/skills/n4h/SKILL.md",
        "home/.agents/skills/n4h/SKILL.md",
        "config/opencode/skills/n4h/SKILL.md",
    ] {
        assert_eq!(fs::read_to_string(f.path(file)).unwrap(), SKILL);
    }
    assert!(!f.path("project/.n4h").exists());
    let again = f.run(
        &[
            "setup",
            "--harness",
            "codex,codex",
            "--scope",
            "user",
            "--yes",
        ],
        0,
    );
    assert_eq!(again["skills"].as_array().unwrap().len(), 1);
    assert_eq!(again["skills"][0]["status"], "unchanged");
}

#[test]
fn project_scope_preserves_instructions_permissions_and_unrelated_skills() {
    let f = Fixture::new();
    for file in [
        "AGENTS.md",
        "CLAUDE.md",
        "opencode.json",
        ".claude/settings.json",
        ".agents/skills/other/SKILL.md",
    ] {
        let path = f.path(&format!("project/{file}"));
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "original").unwrap();
    }
    f.run(
        &[
            "setup",
            "--harness",
            "claude-code,codex,opencode",
            "--scope",
            "project",
            "--yes",
        ],
        0,
    );
    for prefix in [".claude", ".agents", ".opencode"] {
        assert_eq!(
            fs::read_to_string(f.path(&format!("project/{prefix}/skills/n4h/SKILL.md"))).unwrap(),
            SKILL
        );
    }
    for file in [
        "AGENTS.md",
        "CLAUDE.md",
        "opencode.json",
        ".claude/settings.json",
        ".agents/skills/other/SKILL.md",
    ] {
        assert_eq!(
            fs::read_to_string(f.path(&format!("project/{file}"))).unwrap(),
            "original"
        );
    }
    assert_eq!(fs::read_dir(f.path("home")).unwrap().count(), 0);
    assert!(!f.path("project/.n4h").exists());
}

#[test]
fn conflicting_skill_preflights_all_targets_and_never_overwrites() {
    let f = Fixture::new();
    let skill = f.path("project/.agents/skills/n4h/SKILL.md");
    fs::create_dir_all(skill.parent().unwrap()).unwrap();
    fs::write(&skill, "user-authored skill").unwrap();
    let error = f.run(
        &[
            "setup",
            "--harness",
            "claude-code,codex",
            "--scope",
            "project",
            "--yes",
        ],
        3,
    );
    assert_eq!(error["error"]["code"], "skill_exists");
    assert!(!f.path("project/.claude").exists());
    assert_eq!(fs::read_to_string(skill).unwrap(), "user-authored skill");
}

#[test]
fn dry_run_and_budget_refusal_have_no_filesystem_effects() {
    let f = Fixture::new();
    let result = f.run(
        &[
            "setup",
            "--harness",
            "codex",
            "--scope",
            "project",
            "--dry-run",
        ],
        0,
    );
    assert_eq!(result["skills"][0]["status"], "would_install");
    assert!(!f.path("project/.agents").exists());
    let long_project = f.path(&format!(
        "long/{}/{}",
        "abcdef0123456789".repeat(12),
        "abcdef0123456789".repeat(12)
    ));
    fs::create_dir_all(&long_project).unwrap();
    let out = f
        .command()
        .args([
            "setup",
            "--harness",
            "claude-code,codex,opencode",
            "--scope",
            "project",
            "--project",
            long_project.to_str().unwrap(),
            "--yes",
            "--max-tokens",
            "256",
            "--tokenizer",
            "o200k_base",
        ])
        .output()
        .unwrap();
    let result = decode(out, 5);
    assert_eq!(result["error"]["code"], "output_budget_too_small");
    assert_eq!(fs::read_dir(f.path("project")).unwrap().count(), 0);
    assert_eq!(fs::read_dir(long_project).unwrap().count(), 0);
}

#[test]
fn unattended_setup_requires_explicit_harness_and_scope() {
    let f = Fixture::new();
    for args in [
        vec!["setup", "--yes"],
        vec!["setup", "--scope", "user", "--yes"],
        vec!["setup", "--harness", "codex", "--dry-run"],
    ] {
        assert_eq!(f.run(&args, 2)["error"]["code"], "invalid_arguments");
    }
    assert_eq!(fs::read_dir(f.path("project")).unwrap().count(), 0);
}

#[test]
fn explicit_project_and_standard_config_fallback_are_supported() {
    let f = Fixture::new();
    let project = f.path("elsewhere");
    fs::create_dir(&project).unwrap();
    f.run(
        &[
            "setup",
            "--harness",
            "opencode",
            "--scope",
            "project",
            "--project",
            project.to_str().unwrap(),
            "--yes",
        ],
        0,
    );
    assert!(project.join(".opencode/skills/n4h/SKILL.md").exists());
    let output = f
        .command()
        .env_remove("XDG_CONFIG_HOME")
        .args(["setup", "--harness", "opencode", "--scope", "user", "--yes"])
        .output()
        .unwrap();
    decode(output, 0);
    assert!(f.path("home/.config/opencode/skills/n4h/SKILL.md").exists());
    assert_eq!(
        f.run(
            &[
                "setup",
                "--harness",
                "codex",
                "--scope",
                "user",
                "--project",
                ".",
                "--yes"
            ],
            2
        )["error"]["code"],
        "invalid_arguments"
    );
}

#[test]
fn relative_environment_paths_are_rejected() {
    let f = Fixture::new();
    let output = f
        .command()
        .env("XDG_CONFIG_HOME", "relative")
        .args(["setup", "--harness", "opencode", "--scope", "user", "--yes"])
        .output()
        .unwrap();
    assert_eq!(decode(output, 2)["error"]["code"], "invalid_arguments");
}

#[test]
fn installation_commands_bypass_even_invalid_repository_state() {
    let f = Fixture::new();
    fs::write(f.path("project/.n4h"), "not a state directory").unwrap();
    assert_eq!(f.run(&["doctor"], 0)["kind"], "doctor");
    f.run(
        &["setup", "--harness", "codex", "--scope", "project", "--yes"],
        0,
    );
    assert_eq!(
        fs::read_to_string(f.path("project/.n4h")).unwrap(),
        "not a state directory"
    );
}

#[test]
fn doctor_is_read_only_reports_missing_and_different_skills_and_checks_path() {
    let f = Fixture::new();
    let output = f
        .command()
        .env("PATH", Path::new(BIN).parent().unwrap())
        .args(["doctor"])
        .output()
        .unwrap();
    let result = decode(output, 0);
    assert_eq!(result["kind"], "doctor");
    assert_eq!(result["path_matches_running_binary"], true);
    assert_eq!(result["skills"].as_array().unwrap().len(), 6);
    assert!(
        result["skills"]
            .as_array()
            .unwrap()
            .iter()
            .all(|s| s["status"] == "missing")
    );
    assert_eq!(fs::read_dir(f.path("project")).unwrap().count(), 0);
    f.run(
        &["setup", "--harness", "codex", "--scope", "user", "--yes"],
        0,
    );
    fs::write(f.path("home/.agents/skills/n4h/SKILL.md"), "different").unwrap();
    let report = f.run(&["doctor"], 0);
    assert!(
        report["skills"]
            .as_array()
            .unwrap()
            .iter()
            .any(|s| s["status"] == "different")
    );
}

#[cfg(unix)]
#[test]
fn symlink_files_and_parents_including_dangling_links_are_never_followed() {
    use std::os::unix::fs::symlink;
    for parent in [true, false] {
        let f = Fixture::new();
        let outside = f.path("outside");
        fs::create_dir(&outside).unwrap();
        if parent {
            symlink(&outside, f.path("project/.agents")).unwrap();
        } else {
            fs::create_dir_all(f.path("project/.agents/skills/n4h")).unwrap();
            symlink(
                outside.join("missing"),
                f.path("project/.agents/skills/n4h/SKILL.md"),
            )
            .unwrap();
        }
        assert_eq!(
            f.run(
                &["setup", "--harness", "codex", "--scope", "project", "--yes"],
                3
            )["error"]["code"],
            "unsafe_setup_path"
        );
        assert_eq!(fs::read_dir(outside).unwrap().count(), 0);
        let report = f.run(&["doctor"], 0);
        assert!(
            report["skills"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["status"] == "unsafe_or_unreadable")
        );
    }
}
