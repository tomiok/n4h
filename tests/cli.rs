use redb::{
    Database, Durability, ReadableDatabase, ReadableTable, ReadableTableMetadata, TableDefinition,
};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_n4h");

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn empty() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn new() -> Self {
        let fixture = Self::empty();
        fixture.ok(&["init"]);
        fixture
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn write(&self, path: &str, bytes: impl AsRef<[u8]>) {
        let full = self.root().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, bytes).unwrap();
    }

    fn read(&self, path: &str) -> Vec<u8> {
        fs::read(self.root().join(path)).unwrap()
    }

    fn rewrite_json(&self, name: &str, id: &str, change: impl FnOnce(&mut Value)) {
        let database = Database::open(self.root().join(".n4h/state.redb")).unwrap();
        let mut transaction = database.begin_write().unwrap();
        transaction.set_durability(Durability::Immediate).unwrap();
        {
            let mut table = transaction
                .open_table(TableDefinition::<&str, &str>::new(name))
                .unwrap();
            let mut value: Value =
                serde_json::from_str(table.get(id).unwrap().unwrap().value()).unwrap();
            change(&mut value);
            table
                .insert(id, serde_json::to_string(&value).unwrap().as_str())
                .unwrap();
        }
        transaction.commit().unwrap();
    }

    fn table_len(&self, name: &str) -> u64 {
        let database = Database::open(self.root().join(".n4h/state.redb")).unwrap();
        let transaction = database.begin_read().unwrap();
        transaction
            .open_table(TableDefinition::<&str, &str>::new(name))
            .unwrap()
            .len()
            .unwrap()
    }

    fn command(&self) -> Command {
        let mut command = Command::new(BIN);
        command
            .current_dir(self.root())
            .env_remove("N4H_TEST_FAILPOINT");
        command
    }

    fn run(&self, args: &[&str]) -> Output {
        self.command().args(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> Value {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?}: {}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        let value = decode(&out);
        assert_eq!(value["ok"], true);
        value
    }

    fn error(&self, args: &[&str], code: &str, exit: i32) -> Value {
        let out = self.run(args);
        assert_eq!(
            out.status.code(),
            Some(exit),
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stdout)
        );
        let value = decode(&out);
        assert_eq!(value["ok"], false);
        assert_eq!(value["error"]["code"], code);
        value
    }

    fn find(&self, pattern: &str) -> Value {
        self.ok(&["find", pattern])
    }

    fn plan(&self, pattern: &str, replacement: &str) -> Value {
        let result = self.find(pattern);
        self.ok(&[
            "edit",
            "--on",
            string(&result, "result_id"),
            "--scope",
            "all",
            "--replace",
            replacement,
        ])
    }

    #[cfg(feature = "fault-injection")]
    fn crash(&self, args: &[&str], point: &str) {
        let output = self
            .command()
            .args(args)
            .env("N4H_TEST_FAILPOINT", point)
            .output()
            .unwrap();
        assert_eq!(
            output.status.code(),
            Some(86),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}

fn decode(output: &Output) -> Value {
    assert!(output.stdout.len() <= 64 * 1024);
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&output.stdout)))
}

fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap()
}

#[path = "cases/multifile.rs"]
mod multifile;

#[path = "cases/memory.rs"]
mod memory;

#[path = "cases/ast.rs"]
mod ast;

#[path = "cases/index.rs"]
mod index;

#[path = "cases/budget.rs"]
mod budget;

#[test]
fn init_is_explicit_idempotent_and_does_not_edit_user_configuration() {
    let f = Fixture::empty();
    f.write(".gitignore", "existing-rule\n");
    f.write("AGENTS.md", "User instructions\n");
    f.error(&["find", "x"], "not_initialized", 2);
    let first = f.ok(&["init"]);
    let second = f.ok(&["init"]);
    assert_eq!(first["repository_id"], second["repository_id"]);
    assert_eq!(f.read(".gitignore"), b"existing-rule\n");
    assert_eq!(f.read("AGENTS.md"), b"User instructions\n");
    assert!(!f.root().join("CLAUDE.md").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(f.root().join(".n4h"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(f.root().join(".n4h/state.redb"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn full_workflow_preserves_preexisting_work_and_retries_are_idempotent() {
    let f = Fixture::new();
    let before = b"// existing user change\nold old\n";
    f.write("client.rs", before);
    let plan = f.plan("old", "new");
    assert_eq!(f.read("client.rs"), before);
    assert_eq!(plan["sources_modified"], false);
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("client.rs"), b"// existing user change\nnew new\n");
    let retry = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(retry["operation_id"], applied["operation_id"]);
    assert_eq!(retry["idempotent_replay"], true);
    assert_eq!(retry["files_modified_this_call"], 0);
    let undone = f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("client.rs"), before);
    let retried_undo = f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(undone["operation_id"], retried_undo["operation_id"]);
    assert_eq!(retried_undo["idempotent_replay"], true);
}

#[test]
fn stale_result_is_rejected_before_a_plan_is_created() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let result = f.find("old");
    f.write("a.txt", "external old\n");
    f.error(
        &[
            "edit",
            "--on",
            string(&result, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new",
        ],
        "file_changed",
        3,
    );
    assert_eq!(f.read("a.txt"), b"external old\n");
    assert_eq!(f.ok(&["status"])["operations_recorded"], 0);
}

#[test]
fn changed_preview_is_rejected_before_checkpoint_or_write() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let plan = f.plan("old", "new");
    f.write("a.txt", "someone else\n");
    f.error(&["apply", string(&plan, "plan_id")], "file_changed", 3);
    assert_eq!(f.read("a.txt"), b"someone else\n");
    assert_eq!(f.ok(&["status"])["operations_recorded"], 0);
}

#[test]
fn undo_refuses_to_discard_external_edits_or_recreate_a_missing_file() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let plan = f.plan("old", "new");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    f.write("a.txt", "external\n");
    f.error(
        &["undo", string(&applied, "checkpoint_id")],
        "file_changed",
        3,
    );
    assert_eq!(f.read("a.txt"), b"external\n");
    fs::remove_file(f.root().join("a.txt")).unwrap();
    f.error(
        &["undo", string(&applied, "checkpoint_id")],
        "file_changed",
        3,
    );
    assert!(!f.root().join("a.txt").exists());
}

#[test]
fn pagination_is_stable_and_selection_never_means_only_the_visible_page() {
    let f = Fixture::new();
    f.write("a.txt", "old old old old old\n");
    let page = f.ok(&["find", "old", "--page-size", "2"]);
    let result_id = string(&page, "result_id");
    assert_eq!(page["total_matches"], 5);
    assert_eq!(page["shown_matches"], 2);
    assert_eq!(page["truncated"], true);
    let next = f.ok(&[
        "inspect",
        result_id,
        "--cursor",
        string(&page, "cursor_next"),
        "--page-size",
        "2",
    ]);
    let last = f.ok(&[
        "inspect",
        result_id,
        "--cursor",
        string(&next, "cursor_next"),
        "--page-size",
        "2",
    ]);
    let ids: Vec<_> = [&page, &next, &last]
        .iter()
        .flat_map(|p| {
            p["matches"]
                .as_array()
                .unwrap()
                .iter()
                .map(|m| m["match_id"].as_str().unwrap())
        })
        .collect();
    assert_eq!(
        ids,
        ["m_000001", "m_000002", "m_000003", "m_000004", "m_000005"]
    );
    assert!(last["cursor_next"].is_null());
    f.error(
        &["edit", "--on", result_id, "--replace", "new"],
        "scope_required",
        2,
    );
    let plan = f.ok(&[
        "edit",
        "--on",
        result_id,
        "--scope",
        "all",
        "--replace",
        "new",
    ]);
    assert_eq!(plan["selected_matches"], 5);
    f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"new new new new new\n");
}

#[test]
fn explicit_subset_and_multifile_scope() {
    let f = Fixture::new();
    f.write("a.txt", "old old\n");
    f.write("b.txt", "old\n");
    let result = f.find("old");
    let id = string(&result, "result_id");
    let all_plan = f.ok(&["edit", "--on", id, "--scope", "all", "--replace", "new"]);
    assert_eq!(all_plan["files_total"], 2);
    let applied = f.ok(&["apply", string(&all_plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"new new\n");
    assert_eq!(f.read("b.txt"), b"new\n");
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    let plan = f.ok(&[
        "edit",
        "--on",
        id,
        "--match-id",
        "m_000002",
        "--replace",
        "new",
    ]);
    f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"old new\n");
    assert_eq!(f.read("b.txt"), b"old\n");
}

#[test]
fn invalid_duplicate_or_ambiguous_selections_are_rejected() {
    let f = Fixture::new();
    f.write("a", "old old");
    let result = f.find("old");
    let id = string(&result, "result_id");
    f.error(
        &[
            "edit",
            "--on",
            id,
            "--match-id",
            "m_unknown",
            "--replace",
            "x",
        ],
        "invalid_arguments",
        2,
    );
    f.error(
        &[
            "edit",
            "--on",
            id,
            "--match-id",
            "m_000001",
            "--match-id",
            "m_000001",
            "--replace",
            "x",
        ],
        "invalid_arguments",
        2,
    );
    f.error(
        &[
            "edit",
            "--on",
            id,
            "--match-id",
            "m_000001",
            "--scope",
            "all",
            "--replace",
            "x",
        ],
        "invalid_arguments",
        2,
    );
    f.error(
        &["edit", "--on", id, "--scope", "all", "--replace", "old"],
        "no_changes",
        3,
    );
    let plan = f.ok(&[
        "edit",
        "--on",
        id,
        "--match-id",
        "m_000001",
        "--match-id",
        "m_000002",
        "--replace",
        "x",
    ]);
    assert_eq!(plan["selected_matches"], 2);
}

#[test]
fn unicode_byte_ranges_bom_crlf_and_permissions_survive_roundtrip() {
    let f = Fixture::new();
    let before = "\u{feff}é🙂 old old\r\nkeep\r\n";
    f.write("ruta con espacios/日本語.rs", before);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            f.root().join("ruta con espacios/日本語.rs"),
            fs::Permissions::from_mode(0o640),
        )
        .unwrap();
    }
    let result = f.find("old");
    assert_eq!(result["matches"][0]["start_byte"], 10);
    assert_eq!(result["matches"][0]["column_byte"], 11);
    let plan = f.ok(&[
        "edit",
        "--on",
        string(&result, "result_id"),
        "--scope",
        "all",
        "--replace",
        "nuevo🙂",
    ]);
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(
        f.read("ruta con espacios/日本語.rs"),
        "\u{feff}é🙂 nuevo🙂 nuevo🙂\r\nkeep\r\n".as_bytes()
    );
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("ruta con espacios/日本語.rs"), before.as_bytes());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(f.root().join("ruta con espacios/日本語.rs"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o640
        );
    }
}

#[test]
fn regex_search_has_literal_replacement_and_rejects_zero_width_matches() {
    let f = Fixture::new();
    f.write("a.txt", "v12 v34\n");
    assert_eq!(f.find("v[0-9]+")["total_matches"], 0);
    let result = f.ok(&["find", "v[0-9]+", "--regex"]);
    assert_eq!(result["total_matches"], 2);
    let plan = f.ok(&[
        "edit",
        "--on",
        string(&result, "result_id"),
        "--scope",
        "all",
        "--replace",
        "$1\\n",
    ]);
    f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"$1\\n $1\\n\n");
    f.error(&["find", "a*", "--regex"], "invalid_arguments", 2);
    f.error(&["find", "\\b", "--regex"], "invalid_arguments", 2);
    f.error(&["find", "[", "--regex"], "invalid_arguments", 2);
    f.error(&["find", "one\ntwo"], "invalid_arguments", 2);
}

#[test]
fn replacement_file_is_materialized_before_apply_and_stdin_is_supported() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let replacement = tempfile::NamedTempFile::new().unwrap();
    fs::write(replacement.path(), "first\nsecond").unwrap();
    let result = f.find("old");
    let plan = f.ok(&[
        "edit",
        "--on",
        string(&result, "result_id"),
        "--scope",
        "all",
        "--replacement-file",
        replacement.path().to_str().unwrap(),
    ]);
    fs::write(replacement.path(), "changed after preview").unwrap();
    f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"first\nsecond\n");
    let result = f.find("first");
    let mut child = f
        .command()
        .args([
            "edit",
            "--on",
            string(&result, "result_id"),
            "--scope",
            "all",
            "--replacement-stdin",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"stdin text")
        .unwrap();
    let plan = decode(&child.wait_with_output().unwrap());
    f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"stdin text\nsecond\n");
}

#[test]
fn ignore_rules_hidden_files_and_local_state_are_not_searched() {
    let f = Fixture::new();
    fs::create_dir(f.root().join(".git")).unwrap();
    f.write(".gitignore", "ignored.txt\n");
    f.write("ignored.txt", "needle");
    f.write(".hidden", "needle");
    f.write(".git/description", "needle");
    f.write(".n4h/private.txt", "needle");
    f.write("visible.txt", "needle");
    let result = f.find("needle");
    assert_eq!(result["total_matches"], 1);
    assert_eq!(result["matches"][0]["path"], "visible.txt");
    assert_eq!(f.find("needle")["total_matches"], 1);
}

#[test]
fn binary_large_and_readonly_files_are_handled_explicitly() {
    let f = Fixture::new();
    f.write("binary.dat", b"needle\0needle");
    f.write("big.txt", vec![b'x'; 2 * 1024 * 1024 + 1]);
    f.write("a.txt", "needle");
    let result = f.find("needle");
    assert_eq!(result["total_matches"], 1);
    assert_eq!(result["skipped_total"], 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(f.root().join("a.txt"), fs::Permissions::from_mode(0o444)).unwrap();
        f.error(
            &[
                "edit",
                "--on",
                string(&result, "result_id"),
                "--scope",
                "all",
                "--replace",
                "x",
            ],
            "unsupported_file",
            5,
        );
    }
}

#[cfg(unix)]
#[test]
fn symlinks_hardlinks_and_outside_paths_are_not_edit_targets() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let external = tempfile::tempdir().unwrap();
    fs::write(external.path().join("outside.txt"), "needle").unwrap();
    symlink(
        external.path().join("outside.txt"),
        f.root().join("link.txt"),
    )
    .unwrap();
    symlink(external.path(), f.root().join("linked-dir")).unwrap();
    f.write("a.txt", "needle");
    fs::hard_link(f.root().join("a.txt"), f.root().join("hardlink.txt")).unwrap();
    let result = f.find("needle");
    assert_eq!(result["total_matches"], 0);
    f.error(&["find", "needle", "link.txt"], "unsupported_file", 5);
    f.error(
        &["find", "needle", "linked-dir/outside.txt"],
        "unsupported_file",
        5,
    );
    f.error(
        &["find", "needle", external.path().to_str().unwrap()],
        "invalid_arguments",
        2,
    );
    assert_eq!(
        fs::read(external.path().join("outside.txt")).unwrap(),
        b"needle"
    );
}

#[test]
fn inspect_context_checks_freshness_but_persisted_results_do_not_repeat_search() {
    let f = Fixture::new();
    f.write("a.txt", "before\nold\nafter\n");
    let result = f.find("old");
    let id = string(&result, "result_id");
    let context = f.ok(&["inspect", id, "--match-id", "m_000001", "--context", "1"]);
    assert_eq!(context["context"].as_array().unwrap().len(), 3);
    f.write("a.txt", "different\n");
    assert_eq!(f.ok(&["inspect", id])["total_matches"], 1);
    f.error(
        &["inspect", id, "--match-id", "m_000001"],
        "file_changed",
        3,
    );
}

#[test]
fn cursors_cannot_be_reused_for_another_result_and_paths_are_deduplicated() {
    let f = Fixture::new();
    f.write("a.txt", "old old old");
    let a = f.ok(&["find", "old", ".", "a.txt", "--page-size", "1"]);
    assert_eq!(a["total_matches"], 3);
    let b = f.find("old");
    f.error(
        &[
            "inspect",
            string(&b, "result_id"),
            "--cursor",
            string(&a, "cursor_next"),
        ],
        "invalid_cursor",
        3,
    );
    f.error(
        &["inspect", string(&a, "result_id"), "--cursor", "garbage"],
        "invalid_cursor",
        3,
    );
}

#[test]
fn commands_work_from_subdirectories_and_search_scope_stays_local() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    f.write("src/b.txt", "old");
    let output = Command::new(BIN)
        .current_dir(f.root().join("src"))
        .args(["find", "old"])
        .output()
        .unwrap();
    let result = decode(&output);
    assert_eq!(result["total_matches"], 1);
    assert_eq!(result["matches"][0]["path"], "src/b.txt");
    let output = Command::new(BIN)
        .current_dir(f.root().join("src"))
        .arg("init")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(!f.root().join("src/.n4h").exists());
}

#[test]
fn result_and_unapplied_plan_expire_but_committed_checkpoint_does_not() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let result = f.find("old");
    let plan = f.ok(&[
        "edit",
        "--on",
        string(&result, "result_id"),
        "--scope",
        "all",
        "--replace",
        "new",
    ]);
    f.rewrite_json("results", string(&result, "result_id"), |v| {
        v["created_at"] = 0.into()
    });
    f.error(
        &["inspect", string(&result, "result_id")],
        "result_expired",
        3,
    );
    f.rewrite_json("plans", string(&plan, "plan_id"), |v| {
        v["created_at"] = 0.into()
    });
    f.error(&["apply", string(&plan, "plan_id")], "plan_expired", 3);
    let plan = f.plan("old", "new");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    f.rewrite_json("operations", string(&applied, "operation_id"), |v| {
        v["operation"]["created_at"] = 0.into()
    });
    f.rewrite_json("plans", string(&plan, "plan_id"), |v| {
        v["created_at"] = 0.into()
    });
    f.ok(&["apply", string(&plan, "plan_id")]);
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.txt"), b"old");
}

#[test]
fn oversized_match_set_does_not_create_an_editable_handle() {
    let f = Fixture::new();
    f.write("many.txt", "x ".repeat(20_001));
    f.error(&["find", "x"], "search_limit_exceeded", 5);
    assert_eq!(f.table_len("results"), 0);
}

#[test]
fn storage_quota_rejects_new_state_without_deleting_checkpoints() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "new");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    let padding = fs::File::create(f.root().join(".n4h/padding")).unwrap();
    padding.set_len(201 * 1024 * 1024).unwrap();
    f.error(&["find", "new"], "storage_limit_exceeded", 5);
    assert_eq!(f.read("a.txt"), b"new");
    fs::remove_file(f.root().join(".n4h/padding")).unwrap();
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.txt"), b"old");
}

#[test]
fn diff_pagination_is_valid_json_and_reconstructs_the_reviewed_diff() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let replacement = tempfile::NamedTempFile::new().unwrap();
    fs::write(replacement.path(), "🙂\"\\\t".repeat(12_000)).unwrap();
    let result = f.find("old");
    let plan = f.ok(&[
        "edit",
        "--on",
        string(&result, "result_id"),
        "--scope",
        "all",
        "--replacement-file",
        replacement.path().to_str().unwrap(),
    ]);
    assert_eq!(plan["diff_truncated"], true);
    let mut diff = string(&plan, "diff").to_owned();
    let mut page = plan.clone();
    let mut pages = 0;
    while let Some(cursor) = page["cursor_next"].as_str() {
        page = f.ok(&["inspect", string(&plan, "plan_id"), "--cursor", cursor]);
        diff.push_str(string(&page, "diff"));
        pages += 1;
        assert!(pages < 100);
    }
    assert!(diff.contains("-old"));
    assert!(diff.contains(&"🙂\"\\\t".repeat(12_000)));
    assert_eq!(f.read("a.txt"), b"old\n");
}

#[test]
fn result_pages_shrink_to_byte_budget_without_omitting_matches() {
    use std::collections::BTreeSet;
    let f = Fixture::new();
    f.write("a.txt", format!("{}\n", "🙂".repeat(160)).repeat(500));
    let mut page = f.ok(&["find", ".+", "--regex", "--page-size", "500"]);
    assert_eq!(page["total_matches"], 500);
    assert!(page["shown_matches"].as_u64().unwrap() < 500);
    let id = string(&page, "result_id").to_owned();
    let mut seen = BTreeSet::new();
    loop {
        for found in page["matches"].as_array().unwrap() {
            assert!(seen.insert(string(found, "match_id").to_owned()));
        }
        let Some(cursor) = page["cursor_next"].as_str() else {
            break;
        };
        page = f.ok(&["inspect", &id, "--cursor", cursor, "--page-size", "500"]);
    }
    assert_eq!(seen.len(), 500);
}

#[test]
fn copied_state_and_cross_repository_handles_are_rejected() {
    let a = Fixture::new();
    a.write("a.txt", "old");
    let result = a.find("old");
    let b = Fixture::new();
    b.error(
        &["inspect", string(&result, "result_id")],
        "result_not_found",
        3,
    );
    let c = Fixture::empty();
    fs::create_dir(c.root().join(".n4h")).unwrap();
    fs::copy(
        a.root().join(".n4h/state.redb"),
        c.root().join(".n4h/state.redb"),
    )
    .unwrap();
    c.error(&["status"], "invalid_state", 3);
}

#[test]
fn repository_lock_serializes_processes() {
    use fs2::FileExt;
    let f = Fixture::new();
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(f.root().join(".n4h/lock"))
        .unwrap();
    lock.lock_exclusive().unwrap();
    f.error(&["status"], "repository_busy", 3);
    FileExt::unlock(&lock).unwrap();
    f.ok(&["status"]);
}

#[test]
fn invalid_cli_empty_search_and_dash_patterns_have_stable_contracts() {
    let f = Fixture::new();
    f.write("a.txt", "-old");
    f.error(&["nonsense"], "invalid_arguments", 2);
    f.error(&["find", "old", "--page-size", "0"], "invalid_arguments", 2);
    f.error(&["recover", "--rollback"], "invalid_arguments", 2);
    let result = f.find("absent");
    assert_eq!(result["total_matches"], 0);
    assert_eq!(result["truncated"], false);
    assert_eq!(f.ok(&["find", "--json", "--", "-old"])["total_matches"], 1);
    assert!(f.run(&["--help"]).status.success());
    assert!(f.run(&["--version"]).status.success());
}

#[test]
fn malformed_ignore_rules_report_incomplete_coverage_and_block_planning() {
    let f = Fixture::new();
    fs::create_dir(f.root().join(".git")).unwrap();
    f.write(".gitignore", "[z-a]\n");
    f.write("a.txt", "needle");
    let result = f.find("needle");
    assert_eq!(result["coverage_complete"], false);
    assert_eq!(result["total_matches"], 1);
    f.error(
        &[
            "edit",
            "--on",
            string(&result, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new",
        ],
        "incomplete_coverage",
        3,
    );
    assert_eq!(f.read("a.txt"), b"needle");
}

#[test]
fn context_near_start_does_not_show_extra_lines_after_the_radius() {
    let f = Fixture::new();
    f.write("a.txt", "needle\n2\n3\n4\n5\n6\n7\n8\n");
    let result = f.find("needle");
    let context = f.ok(&[
        "inspect",
        string(&result, "result_id"),
        "--match-id",
        "m_000001",
        "--context",
        "2",
    ]);
    assert_eq!(context["context"].as_array().unwrap().len(), 3);
    let exact = f.ok(&[
        "inspect",
        string(&result, "result_id"),
        "--match-id",
        "m_000001",
        "--context",
        "0",
    ]);
    assert_eq!(exact["context"].as_array().unwrap().len(), 1);
}

#[test]
fn even_errors_echoing_large_arguments_remain_bounded_json() {
    let f = Fixture::new();
    let huge_flag = format!("--{}", "z".repeat(70_000));
    f.error(&[&huge_flag], "invalid_arguments", 2);
}

#[test]
fn corrupt_stored_plan_bytes_are_not_applied() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "new");
    {
        let db = Database::open(f.root().join(".n4h/state.redb")).unwrap();
        let transaction = db.begin_write().unwrap();
        {
            let mut table = transaction
                .open_table(TableDefinition::<&str, &[u8]>::new("snapshots"))
                .unwrap();
            let key = format!("{}:0:after", string(&plan, "plan_id"));
            table.insert(key.as_str(), b"corrupt".as_slice()).unwrap();
        }
        transaction.commit().unwrap();
    }
    f.error(&["apply", string(&plan, "plan_id")], "invalid_state", 4);
    assert_eq!(f.read("a.txt"), b"old");
    assert_eq!(f.ok(&["status"])["operations_recorded"], 0);
}

#[cfg(unix)]
#[test]
fn externally_changed_permission_bits_are_a_conflict() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    f.write("a.txt", "old");
    fs::set_permissions(f.root().join("a.txt"), fs::Permissions::from_mode(0o644)).unwrap();
    let plan = f.plan("old", "new");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    fs::set_permissions(f.root().join("a.txt"), fs::Permissions::from_mode(0o600)).unwrap();
    f.error(
        &["undo", string(&applied, "checkpoint_id")],
        "file_changed",
        3,
    );
    assert_eq!(f.read("a.txt"), b"new");
    assert_eq!(
        fs::metadata(f.root().join("a.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[cfg(unix)]
#[test]
fn local_state_cannot_be_redirected_through_a_symlink() {
    use std::os::unix::fs::symlink;
    let f = Fixture::empty();
    let external = tempfile::tempdir().unwrap();
    symlink(external.path(), f.root().join(".n4h")).unwrap();
    f.error(&["init"], "invalid_state", 3);
    assert_eq!(fs::read_dir(external.path()).unwrap().count(), 0);
}

#[test]
fn legacy_sqlite_database_and_sidecars_are_preserved_without_new_state() {
    for name in [
        "state.sqlite3",
        "state.sqlite3-wal",
        "state.sqlite3-shm",
        "state.sqlite3-journal",
    ] {
        let f = Fixture::empty();
        let path = format!(".n4h/{name}");
        let original = b"legacy data: preserve checkpoints and pending operations";
        f.write(&path, original);
        f.write("a.txt", "old");
        for command in [&["init"][..], &["status"][..], &["find", "old"][..]] {
            f.error(command, "legacy_state_detected", 3);
        }
        assert_eq!(f.read(&path), original);
        assert_eq!(f.read("a.txt"), b"old");
        assert!(!f.root().join(".n4h/state.redb").exists());
        assert!(!f.root().join(".n4h/lock").exists());
        assert_eq!(fs::read_dir(f.root().join(".n4h")).unwrap().count(), 1);
    }
}

#[test]
fn legacy_state_is_not_ignored_when_redb_also_exists() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let before = f.read(".n4h/state.redb");
    f.write(".n4h/state.sqlite3", b"legacy checkpoints");
    f.error(&["init"], "legacy_state_detected", 3);
    f.error(&["status"], "legacy_state_detected", 3);
    assert_eq!(f.read(".n4h/state.redb"), before);
    assert_eq!(f.read(".n4h/state.sqlite3"), b"legacy checkpoints");
    assert_eq!(f.read("a.txt"), b"old");
}

#[test]
fn unsupported_redb_schema_is_not_reinitialized() {
    let f = Fixture::new();
    {
        let db = Database::open(f.root().join(".n4h/state.redb")).unwrap();
        let transaction = db.begin_write().unwrap();
        transaction
            .open_table(TableDefinition::<&str, &str>::new("meta"))
            .unwrap()
            .insert("schema", "future-version")
            .unwrap();
        transaction.commit().unwrap();
    }
    f.error(&["init"], "invalid_state", 3);
    f.error(&["status"], "invalid_state", 3);
    let db = Database::open(f.root().join(".n4h/state.redb")).unwrap();
    let transaction = db.begin_read().unwrap();
    let table = transaction
        .open_table(TableDefinition::<&str, &str>::new("meta"))
        .unwrap();
    assert_eq!(
        table.get("schema").unwrap().unwrap().value(),
        "future-version"
    );
}

#[test]
fn a_missing_redb_database_is_not_silently_created_by_open_or_init() {
    let f = Fixture::empty();
    fs::create_dir(f.root().join(".n4h")).unwrap();
    f.error(&["status"], "storage_error", 4);
    f.error(&["init"], "storage_error", 4);
    assert!(!f.root().join(".n4h/state.redb").exists());
}

#[test]
fn deletion_plans_keep_an_empty_binary_snapshot_and_can_be_undone() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"");
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.txt"), b"old");
}

#[cfg(feature = "fault-injection")]
#[test]
fn interruption_before_checkpoint_commit_leaves_no_partial_operation_or_indexes() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "new");
    f.crash(
        &["apply", string(&plan, "plan_id")],
        "before_checkpoint_commit",
    );
    assert_eq!(f.read("a.txt"), b"old");
    assert_eq!(f.ok(&["status"])["operations_recorded"], 0);
    for table in [
        "operations",
        "checkpoint_index",
        "source_index",
        "pending_operations",
    ] {
        assert_eq!(f.table_len(table), 0);
    }
    {
        let db = Database::open(f.root().join(".n4h/state.redb")).unwrap();
        let transaction = db.begin_read().unwrap();
        assert_eq!(
            transaction
                .open_table(TableDefinition::<&str, &[u8]>::new("snapshots"))
                .unwrap()
                .len()
                .unwrap(),
            2
        );
    }
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(applied["idempotent_replay"], false);
    assert_eq!(f.read("a.txt"), b"new");
    assert_eq!(f.table_len("operations"), 1);
}

#[cfg(feature = "fault-injection")]
#[test]
fn interruption_during_committed_status_keeps_the_pending_index_consistent() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "new");
    f.crash(
        &["apply", string(&plan, "plan_id")],
        "before_committed_status_commit",
    );
    assert_eq!(f.read("a.txt"), b"new");
    let state = f.ok(&["status"]);
    assert_eq!(state["operations_recorded"], 1);
    assert_eq!(state["pending_operations"][0]["status"], "applying");
    let id = state["pending_operations"][0]["operation_id"]
        .as_str()
        .unwrap();
    assert_eq!(f.ok(&["recover", id])["observation"]["state"], "after");
    f.ok(&["recover", id, "--rollback"]);
    assert_eq!(f.read("a.txt"), b"old");
    assert_eq!(f.table_len("pending_operations"), 0);
    for table in ["operations", "checkpoint_index", "source_index"] {
        assert_eq!(f.table_len(table), 1);
    }
    assert_eq!(f.ok(&["recover", id])["status"], "rolled_back");
}

#[cfg(feature = "fault-injection")]
#[test]
fn recovery_after_checkpoint_does_not_touch_unchanged_sources() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "new");
    f.crash(&["apply", string(&plan, "plan_id")], "after_checkpoint");
    assert_eq!(f.read("a.txt"), b"old");
    let state = f.ok(&["recover"]);
    let operation = state["pending_operations"][0]["operation_id"]
        .as_str()
        .unwrap();
    assert_eq!(
        f.ok(&["recover", operation])["observation"]["state"],
        "before"
    );
    let recovered = f.ok(&["recover", operation, "--rollback"]);
    assert_eq!(recovered["sources_modified_this_call"], false);
    assert_eq!(f.ok(&["status"])["recovery_required"], false);
    f.error(
        &["apply", string(&plan, "plan_id")],
        "operation_rolled_back",
        3,
    );
}

#[cfg(feature = "fault-injection")]
#[test]
fn recovery_after_rename_and_during_rollback_is_idempotent() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "new");
    f.crash(&["apply", string(&plan, "plan_id")], "after_rename");
    assert_eq!(f.read("a.txt"), b"new");
    let state = f.ok(&["status"]);
    let operation = state["pending_operations"][0]["operation_id"]
        .as_str()
        .unwrap();
    assert_eq!(
        f.ok(&["recover", operation])["observation"]["state"],
        "after"
    );
    f.error(&["apply", string(&plan, "plan_id")], "recovery_required", 3);
    f.error(
        &[
            "edit",
            "--on",
            string(&plan, "result_id"),
            "--scope",
            "all",
            "--replace",
            "other",
        ],
        "recovery_required",
        3,
    );
    f.crash(
        &["recover", operation, "--rollback"],
        "rollback_after_rename",
    );
    assert_eq!(f.read("a.txt"), b"old");
    let recovered = f.ok(&["recover", operation, "--rollback"]);
    assert_eq!(recovered["status"], "rolled_back");
    assert_eq!(
        f.ok(&["recover", operation, "--rollback"])["idempotent_replay"],
        true
    );
    assert_eq!(f.ok(&["status"])["recovery_required"], false);
}

#[cfg(feature = "fault-injection")]
#[test]
fn recovery_conflict_preserves_external_work_and_missing_files() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "new");
    f.crash(&["apply", string(&plan, "plan_id")], "after_rename");
    let state = f.ok(&["status"]);
    let operation = state["pending_operations"][0]["operation_id"]
        .as_str()
        .unwrap();
    f.write("a.txt", "external");
    f.error(
        &["recover", operation, "--rollback"],
        "recovery_conflict",
        3,
    );
    assert_eq!(f.read("a.txt"), b"external");
    fs::remove_file(f.root().join("a.txt")).unwrap();
    f.error(
        &["recover", operation, "--rollback"],
        "recovery_conflict",
        3,
    );
    assert!(!f.root().join("a.txt").exists());
    assert_eq!(f.ok(&["status"])["recovery_required"], true);
}

#[cfg(feature = "fault-injection")]
#[test]
fn interrupted_undo_can_be_rolled_back_without_losing_its_checkpoint() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let plan = f.plan("old", "new");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    f.crash(&["undo", string(&applied, "checkpoint_id")], "after_rename");
    assert_eq!(f.read("a.txt"), b"old");
    let state = f.ok(&["status"]);
    let operation = state["pending_operations"][0]["operation_id"]
        .as_str()
        .unwrap();
    f.ok(&["recover", operation, "--rollback"]);
    assert_eq!(f.read("a.txt"), b"new");
    assert_eq!(f.ok(&["status"])["recovery_required"], false);
}
