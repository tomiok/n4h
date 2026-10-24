use crate::budget::Budget;
use crate::error::{Error, Result};
use crate::output;
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

const SKILL: &str = include_str!("../skills/n4h/SKILL.md");

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Harness {
    ClaudeCode,
    Codex,
    Opencode,
}

impl Harness {
    fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::Opencode => "opencode",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Scope {
    User,
    Project,
}

#[derive(Args)]
pub struct SetupArgs {
    /// Select one or several harnesses (comma-separated or repeated).
    #[arg(long, value_enum, value_delimiter = ',', action = clap::ArgAction::Append)]
    harness: Vec<Harness>,
    #[arg(long, value_enum)]
    scope: Option<Scope>,
    /// Project directory; defaults to the current directory, not an inferred root.
    #[arg(long, requires = "scope")]
    project: Option<PathBuf>,
    /// Confirm the displayed destinations without prompting. Requires explicit choices.
    #[arg(long)]
    yes: bool,
    /// Preview without writing or requesting confirmation. Requires explicit choices.
    #[arg(long)]
    dry_run: bool,
}

// Read a controlling terminal, never stdin (which can contain a piped script).
struct Terminal(BufReader<File>);

impl Terminal {
    fn open() -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/tty")
            .map_err(|_| {
                Error::new(
                    "confirmation_required",
                    "No controlling terminal. Specify --harness, --scope and --yes (or --dry-run).",
                    2,
                )
            })?;
        Ok(Self(BufReader::new(file)))
    }

    fn ask(&mut self, prompt: &str) -> Result<String> {
        self.0.get_mut().write_all(prompt.as_bytes())?;
        self.0.get_mut().flush()?;
        let mut answer = String::new();
        if self.0.read_line(&mut answer)? == 0 {
            return Err(Error::new(
                "setup_cancelled",
                "Terminal closed; no installation authorized.",
                2,
            ));
        }
        Ok(answer.trim().to_lowercase())
    }
}

fn absolute_env(name: &str) -> Result<Option<PathBuf>> {
    let Some(value) = std::env::var_os(name).filter(|v| !v.is_empty()) else {
        return Ok(None);
    };
    let path = PathBuf::from(value);
    if !path.is_absolute() {
        return Err(Error::invalid(format!("{name} must be an absolute path.")));
    }
    Ok(Some(path))
}

fn destination(harness: Harness, scope: Scope, project: &Path) -> Result<PathBuf> {
    let base = if scope == Scope::Project {
        project.to_path_buf()
    } else {
        absolute_env("HOME")?.ok_or_else(|| Error::invalid("HOME is required for user skills."))?
    };
    let directory = match (harness, scope) {
        (Harness::ClaudeCode, _) => base.join(".claude"),
        (Harness::Codex, _) => base.join(".agents"),
        (Harness::Opencode, Scope::Project) => base.join(".opencode"),
        (Harness::Opencode, Scope::User) => absolute_env("XDG_CONFIG_HOME")?
            .unwrap_or_else(|| base.join(".config"))
            .join("opencode"),
    };
    Ok(directory.join("skills/n4h/SKILL.md"))
}

// Refuse symlinks at every existing component, including dangling links. This is
// a local-user safety check, not a defense against hostile concurrent FS changes.
fn check_path(path: &Path) -> Result<()> {
    for part in path.ancestors() {
        match fs::symlink_metadata(part) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(Error::new(
                    "unsafe_setup_path",
                    "Refusing a symlink in a skill destination.",
                    3,
                )
                .path(&part.to_string_lossy()));
            }
            Ok(meta) if part != path && !meta.is_dir() => {
                return Err(Error::new(
                    "unsafe_setup_path",
                    "A skill parent is not a directory.",
                    3,
                )
                .path(&part.to_string_lossy()));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn state(path: &Path) -> Result<&'static str> {
    check_path(path)?;
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() && meta.len() == SKILL.len() as u64 => {
            Ok(if fs::read(path)? == SKILL.as_bytes() {
                "current"
            } else {
                "different"
            })
        }
        Ok(_) => Ok("different"),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok("missing"),
        Err(e) => Err(e.into()),
    }
}

pub fn run(mut args: SetupArgs, cwd: &Path, budget: &Budget) -> Result<Value> {
    let mut terminal = None;
    if args.harness.is_empty() || args.scope.is_none() {
        if args.yes || args.dry_run {
            return Err(Error::invalid(
                "Specify --harness and --scope with --yes or --dry-run.",
            ));
        }
        let tty = terminal.insert(Terminal::open()?);
        if args.harness.is_empty() {
            let answer = tty.ask("n4h — choose harnesses (comma-separated):\n  1 Claude Code\n  2 Codex\n  3 OpenCode\n  4 All three\n  5 CLI only (skip skills)\nSelection: ")?;
            if matches!(answer.as_str(), "5" | "cli-only" | "none") {
                let mut response = output::success("setup");
                response["skills"] = json!([]);
                response["scope"] = Value::Null;
                response["project_initialized"] = json!(false);
                response["next_action"] = json!(
                    "CLI-only selected; no skill files changed. Run setup later to add harnesses."
                );
                return budget.finish(response);
            }
            for choice in answer.split(',').map(str::trim) {
                match choice {
                    "1" | "claude-code" => args.harness.push(Harness::ClaudeCode),
                    "2" | "codex" => args.harness.push(Harness::Codex),
                    "3" | "opencode" => args.harness.push(Harness::Opencode),
                    "4" | "all" => args.harness.extend([
                        Harness::ClaudeCode,
                        Harness::Codex,
                        Harness::Opencode,
                    ]),
                    _ => {
                        return Err(Error::invalid(
                            "Choose 1, 2, 3, 4, 5 or comma-separated harness names.",
                        ));
                    }
                }
            }
        }
        if args.scope.is_none() {
            args.scope = Some(
                match tty
                    .ask("Scope: 1 User (all projects), 2 This project: ")?
                    .as_str()
                {
                    "1" | "user" => Scope::User,
                    "2" | "project" => Scope::Project,
                    _ => return Err(Error::invalid("Choose user or project scope.")),
                },
            );
        }
    }
    let scope = args.scope.expect("scope selected");
    if scope == Scope::User && args.project.is_some() {
        return Err(Error::invalid("--project requires --scope project."));
    }
    let project = args.project.as_deref().unwrap_or(cwd).canonicalize()?;
    if !project.is_dir() {
        return Err(Error::invalid("Project must be an existing directory."));
    }
    let mut targets = Vec::new();
    for harness in args.harness {
        let path = destination(harness, scope, &project)?;
        if targets.iter().any(|(_, p, _)| p == &path) {
            continue;
        }
        let status = state(&path)?;
        if status == "different" {
            return Err(Error::new("skill_exists", "An existing skill differs; nothing was overwritten. Back it up and move it explicitly before retrying.", 3).path(&path.to_string_lossy()));
        }
        targets.push((harness, path, status));
    }
    let mut response = output::success("setup");
    response["dry_run"] = json!(args.dry_run);
    response["scope"] = json!(if scope == Scope::User {
        "user"
    } else {
        "project"
    });
    response["skills"] = json!(targets.iter().map(|(h, p, s)| json!({"harness": h.name(), "path": p,
        "status": if *s == "current" { "unchanged" } else if args.dry_run { "would_install" } else { "installed" }})).collect::<Vec<_>>());
    response["project_initialized"] = json!(false);
    response["next_action"] = json!(
        "Make n4h available on the harness PATH, then explicitly ask the agent to use the n4h skill. No permissions or instructions were changed."
    );
    // Refuse an unreportable receipt before creating directories or skills.
    let response = budget.finish(response)?;
    for (_, path, status) in &targets {
        eprintln!("n4h: {status}: {}", path.display());
    }
    if args.dry_run {
        return Ok(response);
    }
    if !args.yes {
        if terminal.is_none() {
            terminal = Some(Terminal::open()?);
        }
        let answer = terminal
            .as_mut()
            .expect("terminal open")
            .ask("Install only these skills? [y/N] ")?;
        if answer != "y" && answer != "yes" {
            return Err(Error::new(
                "setup_cancelled",
                "Setup cancelled; no files written.",
                2,
            ));
        }
    }
    for (_, path, status) in &targets {
        if *status == "current" {
            continue;
        }
        check_path(path)?;
        let parent = path.parent().expect("skill parent");
        fs::create_dir_all(parent)?;
        check_path(path)?;
        let mut staged = tempfile::NamedTempFile::new_in(parent)?;
        staged.write_all(SKILL.as_bytes())?;
        staged.as_file().sync_all()?;
        staged.persist_noclobber(path).map_err(|e| Error::new("setup_write_failed",
            format!("Could not create skill without overwriting: {}. Earlier destinations may already be installed; retry is safe.", e.error), 4).path(&path.to_string_lossy()))?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
    }
    Ok(response)
}

pub fn doctor(cwd: &Path, budget: &Budget) -> Result<Value> {
    let executable = std::env::current_exe()?.canonicalize()?;
    let on_path = std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join("n4h"))
            .find(|path| is_executable(path))
    });
    let path_matches = on_path
        .as_ref()
        .and_then(|p| p.canonicalize().ok())
        .as_ref()
        == Some(&executable);
    let mut skills = Vec::new();
    for scope in [Scope::User, Scope::Project] {
        for harness in [Harness::ClaudeCode, Harness::Codex, Harness::Opencode] {
            let path = destination(harness, scope, cwd)?;
            let status = state(&path).unwrap_or("unsafe_or_unreadable");
            skills.push(json!({"harness": harness.name(), "scope": if scope == Scope::User { "user" } else { "project" }, "path": path, "status": status}));
        }
    }
    let mut response = output::success("doctor");
    response["version"] = json!(env!("CARGO_PKG_VERSION"));
    response["executable"] = json!(executable);
    response["path_binary"] = json!(on_path);
    response["path_matches_running_binary"] = json!(path_matches);
    response["skills"] = json!(skills);
    response["notes"] = json!([
        "Missing skills are normal for CLI-only or unselected harnesses. This check does not validate harness permissions or model usage.",
        "OpenCode also discovers .claude/.agents compatibility skills; project or custom skills can override a global n4h skill.",
        "Project checks use the current directory only. No project is initialized or indexed."
    ]);
    budget.finish(response)
}

fn is_executable(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}
