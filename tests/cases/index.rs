use super::{BIN, Fixture, decode, string};
use redb::{Database, ReadableTable, TableDefinition};
use serde_json::{Value, json};
use std::fs;
use std::process::Command;

fn indexed(f: &Fixture, pattern: &str) -> Value {
    f.ok(&["find", pattern, "--indexed"])
}

fn signature(value: &Value) -> Value {
    json!(
        value["matches"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                json!([
                    m["path"],
                    m["start_byte"],
                    m["end_byte"],
                    m["line"],
                    m["column_byte"],
                    m["text"],
                    m["file_hash"]
                ])
            })
            .collect::<Vec<_>>()
    )
}

#[test]
fn index_is_explicit_and_regex_or_ast_are_not_silently_accelerated() {
    let f = Fixture::new();
    assert_eq!(f.ok(&["index", "status"])["ready"], false);
    assert!(!f.root().join(".n4h/index.redb").exists());
    f.error(&["find", "needle", "--indexed"], "index_not_ready", 3);
    f.error(&["index", "update"], "index_not_ready", 3);
    f.error(
        &["find", "needle", "--indexed", "--regex"],
        "invalid_arguments",
        2,
    );
    f.error(
        &["ast", "find", "foo($A)", "--indexed"],
        "invalid_arguments",
        2,
    );
    assert_eq!(f.ok(&["index", "clear"])["removed"], false);
    f.write("a.txt", "needle");
    assert_eq!(f.find("needle")["total_matches"], 1);
    assert!(!f.root().join(".n4h/index.redb").exists());
}

#[test]
fn indexed_matches_equal_live_literal_matches_for_unicode_bom_crlf_short_and_repeated_patterns() {
    let f = Fixture::new();
    f.write(
        "a.txt",
        "\u{feff}old_api old_api\r\né🙂 x ab abababa .+ a\\b\r\n",
    );
    f.write("日本語/b.txt", "nothing old_api\n🙂é\n");
    let built = f.ok(&["index", "build"]);
    assert_eq!(built["ready"], true);
    assert_eq!(built["generation"], 1);
    assert_eq!(built["files_indexed"], 2);
    assert_eq!(built["verification_mode"], "full_hash");
    for pattern in [
        "old_api",
        "é",
        "🙂",
        "x",
        "ab",
        "aba",
        ".+",
        "a\\b",
        "absent",
        "a",
        "日本語",
    ] {
        let live = f.find(pattern);
        let cached = indexed(&f, pattern);
        assert_eq!(signature(&live), signature(&cached), "pattern={pattern}");
        assert_eq!(live["total_matches"], cached["total_matches"]);
        assert_eq!(cached["scope"], "indexed_snapshot");
        assert_eq!(cached["index_generation"], 1);
        assert_eq!(cached["index"]["index_id"], built["index_id"]);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(f.root().join(".n4h/index.redb"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn postings_filter_candidates_but_full_verification_eliminates_false_positives() {
    let f = Fixture::new();
    f.write("a.txt", "rare_target_1234");
    f.write("b.txt", "abc ---- bcd");
    f.write("c.txt", "irrelevant");
    f.ok(&["index", "build"]);
    let found = indexed(&f, "rare_target_1234");
    assert_eq!(found["index"]["files_total"], 3);
    assert_eq!(found["index"]["candidate_files"], 1);
    assert_eq!(found["files_scanned"], 1);
    assert_eq!(found["total_matches"], 1);
    let false_positive = indexed(&f, "abcd");
    assert_eq!(false_positive["index"]["candidate_files"], 1);
    assert_eq!(false_positive["total_matches"], 0);
    let missing = indexed(&f, "zzzzzz");
    assert_eq!(missing["index"]["candidate_files"], 0);
    assert_eq!(missing["files_scanned"], 0);
    assert_eq!(indexed(&f, "a")["index"]["candidate_files"], 3);
}

#[test]
fn incremental_updates_handle_additions_modifications_deletions_and_reuse() {
    let f = Fixture::new();
    f.write("a.txt", "old first");
    f.write("b.txt", "old removed");
    f.write("c.txt", "unchanged");
    let built = f.ok(&["index", "build"]);
    f.write("a.txt", "new changed content");
    fs::remove_file(f.root().join("b.txt")).unwrap();
    f.write("d.txt", "new added");
    let updated = f.ok(&["index", "update"]);
    assert_eq!(updated["generation"], 2);
    assert_eq!(updated["index_id"], built["index_id"]);
    assert_eq!(updated["files_indexed"], 3);
    assert_eq!(updated["counters"]["files_added"], 1);
    assert_eq!(updated["counters"]["files_changed"], 1);
    assert_eq!(updated["counters"]["files_deleted"], 1);
    assert_eq!(updated["counters"]["files_reused"], 1);
    assert_eq!(updated["counters"]["files_read"], 2);
    assert_eq!(indexed(&f, "old")["total_matches"], 0);
    assert_eq!(signature(&indexed(&f, "new")), signature(&f.find("new")));
    let noop = f.ok(&["index", "update"]);
    assert_eq!(noop["generation"], 3);
    assert_eq!(noop["counters"]["files_read"], 0);
    assert_eq!(noop["counters"]["bytes_read"], 0);
    assert_eq!(noop["counters"]["files_reused"], 3);
    let verified = f.ok(&["index", "update", "--verify"]);
    assert_eq!(verified["generation"], 4);
    assert_eq!(verified["verification_mode"], "full_hash");
    assert_eq!(verified["counters"]["files_read"], 3);
    assert_eq!(verified["counters"]["files_changed"], 0);
    assert_eq!(verified["counters"]["files_reused"], 3);
}

#[test]
fn metadata_changes_with_identical_content_refresh_the_stamp_without_rewriting_postings() {
    let f = Fixture::new();
    f.write("a.txt", "identical content");
    let first = f.ok(&["index", "build"]);
    // Replace the inode even if filesystem timestamp resolution is coarse.
    f.write("replacement", "identical content");
    fs::rename(f.root().join("replacement"), f.root().join("a.txt")).unwrap();
    let updated = f.ok(&["index", "update"]);
    assert_eq!(updated["counters"]["files_read"], 1);
    assert_eq!(updated["counters"]["files_reused"], 1);
    assert_eq!(updated["counters"]["files_changed"], 0);
    assert_eq!(updated["posting_pairs"], first["posting_pairs"]);
    assert_eq!(f.ok(&["index", "update"])["counters"]["files_read"], 0);
}

#[test]
fn snapshots_do_not_claim_live_freshness_and_edits_still_check_real_files() {
    let f = Fixture::new();
    f.write("a.txt", "old alpha");
    f.write("b.txt", "old beta");
    f.ok(&["index", "build"]);
    f.write("a.txt", "new alpha");
    fs::remove_file(f.root().join("b.txt")).unwrap();
    f.write("c.txt", "old gamma");
    assert_eq!(f.find("old")["total_matches"], 1);
    let snapshot = indexed(&f, "old");
    assert_eq!(snapshot["total_matches"], 2);
    assert_eq!(snapshot["scope"], "indexed_snapshot");
    assert!(!string(&snapshot, "snapshot_warning").is_empty());
    let deleted_scope = f.ok(&["find", "old", "b.txt", "--indexed"]);
    assert_eq!(deleted_scope["total_matches"], 1);
    f.error(
        &[
            "edit",
            "--on",
            string(&snapshot, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new",
        ],
        "file_changed",
        3,
    );
    f.error(
        &[
            "inspect",
            string(&deleted_scope, "result_id"),
            "--match-id",
            "m_000001",
        ],
        "file_changed",
        3,
    );
    assert_eq!(f.table_len("plans"), 0);
    f.ok(&["index", "update"]);
    assert_eq!(signature(&indexed(&f, "old")), signature(&f.find("old")));
    // Existing results still identify their original generation, not the new cache.
    let inspected = f.ok(&["inspect", string(&snapshot, "result_id")]);
    assert_eq!(inspected["index_generation"], 1);
    assert_eq!(inspected["total_matches"], 2);
}

#[test]
fn paths_and_subdirectory_scopes_use_snapshot_paths_not_filesystem_existence() {
    let f = Fixture::new();
    f.write("root.txt", "needle");
    f.write("src/a.txt", "needle");
    f.write("src2/a.txt", "needle");
    f.ok(&["index", "build"]);
    let scoped = f.ok(&["find", "needle", "src", "src/a.txt", "--indexed"]);
    assert_eq!(scoped["total_matches"], 1);
    assert_eq!(scoped["index"]["scope_files"], 1);
    let output = Command::new(BIN)
        .current_dir(f.root().join("src"))
        .args(["find", "needle", "--indexed"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(decode(&output)["total_matches"], 1);
    let output = Command::new(BIN)
        .current_dir(f.root().join("src"))
        .args(["index", "update"])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(decode(&output)["files_indexed"], 3);
    f.error(
        &["find", "needle", "../", "--indexed"],
        "invalid_arguments",
        2,
    );
    f.error(
        &["find", "needle", ".n4h", "--indexed"],
        "unsupported_file",
        5,
    );
    assert_eq!(
        f.ok(&["find", "needle", "absent", "--indexed"])["total_matches"],
        0
    );
}

#[test]
fn ignore_rules_are_reevaluated_and_policy_omissions_are_visible() {
    let f = Fixture::new();
    fs::create_dir(f.root().join(".git")).unwrap();
    f.write(".gitignore", "ignored.txt\n");
    f.write("ignored.txt", "needle");
    f.write(".hidden", "needle");
    f.write(".n4h/private", "needle");
    f.write("a.txt", "needle");
    f.write("b.txt", "needle");
    f.write("binary", b"needle\0");
    f.write("large", vec![b'x'; 2 * 1024 * 1024 + 1]);
    let built = f.ok(&["index", "build"]);
    assert_eq!(built["files_indexed"], 2);
    assert_eq!(built["skipped_total"], 2);
    assert_eq!(indexed(&f, "needle")["total_matches"], 2);
    f.write(".gitignore", "ignored.txt\nb.txt\n");
    let updated = f.ok(&["index", "update"]);
    assert_eq!(updated["files_indexed"], 1);
    assert_eq!(updated["counters"]["files_deleted"], 1);
    assert_eq!(indexed(&f, "needle")["total_matches"], 1);
    f.write(".gitignore", "b.txt\n");
    f.ok(&["index", "update"]);
    assert_eq!(indexed(&f, "needle")["total_matches"], 2);
}

#[test]
fn a_traversal_or_ignore_error_never_publishes_an_incomplete_generation() {
    let f = Fixture::new();
    fs::create_dir(f.root().join(".git")).unwrap();
    f.write("a.txt", "old");
    let built = f.ok(&["index", "build"]);
    f.write("a.txt", "new");
    f.write(".gitignore", "[z-a]\n");
    f.error(&["index", "update"], "index_scan_failed", 3);
    assert_eq!(
        f.ok(&["index", "status"])["generation"],
        built["generation"]
    );
    assert_eq!(indexed(&f, "old")["total_matches"], 1);
    f.error(&["index", "build"], "index_scan_failed", 3);
    assert_eq!(indexed(&f, "old")["total_matches"], 1);
}

#[test]
fn index_clear_preserves_results_notes_plans_checkpoints_and_idempotency() {
    let f = Fixture::new();
    f.write("a.txt", "old old");
    let note = f.ok(&["mem", "put", "tests", "--text", "cargo test"]);
    let built = f.ok(&["index", "build"]);
    let result = indexed(&f, "old");
    let plan = f.ok(&[
        "edit",
        "--on",
        string(&result, "result_id"),
        "--scope",
        "all",
        "--replace",
        "new",
    ]);
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.ok(&["index", "clear"])["durable_state_modified"], false);
    assert!(!f.root().join(".n4h/index.redb").exists());
    assert_eq!(f.ok(&["mem", "get", "tests"])["note"], note["note"]);
    assert_eq!(
        f.ok(&["inspect", string(&result, "result_id")])["total_matches"],
        2
    );
    assert_eq!(
        f.ok(&["apply", string(&plan, "plan_id")])["idempotent_replay"],
        true
    );
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.txt"), b"old old");
    let rebuilt = f.ok(&["index", "build"]);
    assert_eq!(rebuilt["generation"], 1);
    assert_ne!(rebuilt["index_id"], built["index_id"]);
    assert_eq!(
        f.ok(&["inspect", string(&result, "result_id")])["index"]["index_id"],
        built["index_id"]
    );
}

#[test]
fn indexed_results_page_without_querying_the_cache_again() {
    let f = Fixture::new();
    f.write("a.txt", "old old old old old");
    f.ok(&["index", "build"]);
    let first = f.ok(&["find", "old", "--indexed", "--page-size", "2"]);
    f.ok(&["index", "clear"]);
    let second = f.ok(&[
        "inspect",
        string(&first, "result_id"),
        "--cursor",
        string(&first, "cursor_next"),
        "--page-size",
        "2",
    ]);
    let third = f.ok(&[
        "inspect",
        string(&first, "result_id"),
        "--cursor",
        string(&second, "cursor_next"),
    ]);
    assert_eq!(second["shown_matches"], 2);
    assert_eq!(third["shown_matches"], 1);
    assert_eq!(third["index_generation"], 1);
    assert!(third["cursor_next"].is_null());
}

#[test]
fn quotas_abort_publication_and_cache_size_does_not_consume_the_durable_state_quota() {
    let f = Fixture::new();
    f.write("a.txt", "needle");
    let built = f.ok(&["index", "build"]);
    f.write("a.txt", "new content");
    f.error(
        &["index", "update", "--max-bytes", "1048576"],
        "index_quota_exceeded",
        5,
    );
    assert_eq!(
        f.ok(&["index", "status"])["generation"],
        built["generation"]
    );
    assert_eq!(indexed(&f, "needle")["total_matches"], 1);
    f.ok(&["index", "clear"]);
    let padding = fs::File::create(f.root().join(".n4h/index.redb")).unwrap();
    padding.set_len(201 * 1024 * 1024).unwrap();
    f.ok(&[
        "mem",
        "put",
        "still.available",
        "--text",
        "durable state has a separate quota",
    ]);
    assert_eq!(f.find("new")["total_matches"], 1);
    f.ok(&["index", "clear"]);
    f.ok(&["index", "build"]);
}

#[test]
fn copying_an_index_between_repositories_is_rejected_without_touching_durable_state() {
    let a = Fixture::new();
    let b = Fixture::new();
    a.write("a.txt", "needle");
    a.ok(&["index", "build"]);
    fs::copy(
        a.root().join(".n4h/index.redb"),
        b.root().join(".n4h/index.redb"),
    )
    .unwrap();
    b.error(&["index", "status"], "invalid_index", 3);
    b.error(&["find", "needle", "--indexed"], "invalid_index", 3);
    b.error(&["index", "build"], "invalid_index", 3);
    b.ok(&["mem", "put", "safe", "--text", "unaffected"]);
    b.ok(&["index", "clear"]);
    b.ok(&["index", "build"]);
}

#[test]
fn unsupported_index_schema_requires_explicit_clear_instead_of_silent_reset() {
    let f = Fixture::new();
    f.write("a.txt", "needle");
    f.ok(&["index", "build"]);
    {
        let db = Database::open(f.root().join(".n4h/index.redb")).unwrap();
        let transaction = db.begin_write().unwrap();
        {
            let mut meta = transaction
                .open_table(TableDefinition::<&str, &str>::new("meta"))
                .unwrap();
            let mut snapshot: Value =
                serde_json::from_str(meta.get("snapshot").unwrap().unwrap().value()).unwrap();
            snapshot["schema"] = 999.into();
            meta.insert("snapshot", snapshot.to_string().as_str())
                .unwrap();
        }
        transaction.commit().unwrap();
    }
    f.error(&["index", "status"], "invalid_index", 3);
    f.error(&["index", "update"], "invalid_index", 3);
    f.ok(&["index", "clear"]);
    assert_eq!(f.ok(&["index", "build"])["generation"], 1);
}

#[cfg(unix)]
#[test]
fn symlink_and_hardlink_cache_paths_are_never_opened_or_cleared() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let external = tempfile::NamedTempFile::new().unwrap();
    fs::write(external.path(), "private external data").unwrap();
    symlink(external.path(), f.root().join(".n4h/index.redb")).unwrap();
    for args in [
        &["index", "build"][..],
        &["index", "status"][..],
        &["index", "clear"][..],
        &["find", "private", "--indexed"][..],
    ] {
        f.error(args, "invalid_index", 3);
    }
    assert_eq!(fs::read(external.path()).unwrap(), b"private external data");
    fs::remove_file(f.root().join(".n4h/index.redb")).unwrap();
    fs::hard_link(external.path(), f.root().join(".n4h/index.redb")).unwrap();
    f.error(&["index", "clear"], "invalid_index", 3);
    assert_eq!(fs::read(external.path()).unwrap(), b"private external data");
}

#[cfg(unix)]
#[test]
fn source_symlinks_and_new_hardlinks_are_excluded_including_on_updates() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new();
    let external = tempfile::NamedTempFile::new().unwrap();
    fs::write(external.path(), "external needle").unwrap();
    f.write("a.txt", "needle");
    f.write("b.txt", "needle");
    symlink(external.path(), f.root().join("link.txt")).unwrap();
    let built = f.ok(&["index", "build"]);
    assert_eq!(built["files_indexed"], 2);
    fs::hard_link(f.root().join("a.txt"), f.root().join("hard.txt")).unwrap();
    let updated = f.ok(&["index", "update"]);
    assert_eq!(updated["files_indexed"], 1);
    assert_eq!(updated["counters"]["files_deleted"], 1);
    assert_eq!(indexed(&f, "needle")["total_matches"], 1);
}

#[cfg(unix)]
#[test]
fn ctime_and_identity_detect_same_size_writes_even_if_mtime_is_restored() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let mtime = fs::metadata(f.root().join("a.txt"))
        .unwrap()
        .modified()
        .unwrap();
    f.ok(&["index", "build"]);
    f.write("a.txt", "new");
    let file = fs::OpenOptions::new()
        .write(true)
        .open(f.root().join("a.txt"))
        .unwrap();
    file.set_times(fs::FileTimes::new().set_modified(mtime))
        .unwrap();
    let updated = f.ok(&["index", "update"]);
    assert_eq!(updated["counters"]["files_read"], 1);
    assert_eq!(indexed(&f, "new")["total_matches"], 1);
    assert_eq!(indexed(&f, "old")["total_matches"], 0);
}

#[cfg(feature = "fault-injection")]
#[test]
fn an_interrupted_initial_build_is_not_queryable_and_can_be_rebuilt() {
    let f = Fixture::new();
    f.write("a.txt", "needle");
    f.crash(&["index", "build"], "index_after_file");
    assert_eq!(f.ok(&["index", "status"])["ready"], false);
    f.error(&["find", "needle", "--indexed"], "index_not_ready", 3);
    f.error(&["index", "update"], "index_not_ready", 3);
    assert_eq!(f.ok(&["index", "build"])["generation"], 1);
    assert_eq!(indexed(&f, "needle")["total_matches"], 1);
}

#[cfg(feature = "fault-injection")]
#[test]
fn interrupted_updates_and_rebuilds_keep_the_previous_whole_generation() {
    let f = Fixture::new();
    f.write("a.txt", "old alpha");
    f.write("b.txt", "old beta");
    let built = f.ok(&["index", "build"]);
    f.write("a.txt", "new alpha");
    f.write("b.txt", "new beta");
    for (args, point) in [
        (&["index", "update"][..], "index_after_file"),
        (&["index", "update"][..], "index_before_commit"),
        (&["index", "build"][..], "index_after_file"),
        (&["index", "build"][..], "index_before_commit"),
    ] {
        f.crash(args, point);
        let status = f.ok(&["index", "status"]);
        assert_eq!(status["generation"], built["generation"]);
        assert_eq!(status["index_id"], built["index_id"]);
        assert_eq!(indexed(&f, "old")["total_matches"], 2);
        assert_eq!(indexed(&f, "new")["total_matches"], 0);
    }
    assert_eq!(f.ok(&["index", "update"])["generation"], 2);
    assert_eq!(indexed(&f, "new")["total_matches"], 2);
}

#[cfg(feature = "fault-injection")]
#[test]
fn clearing_or_updating_the_index_does_not_clear_a_pending_source_recovery() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    f.write("b.txt", "old");
    let plan = f.plan("old", "new");
    f.ok(&["index", "build"]);
    f.crash(&["apply", string(&plan, "plan_id")], "after_rename_1");
    let status = f.ok(&["status"]);
    let op = status["pending_operations"][0]["operation_id"]
        .as_str()
        .unwrap();
    f.ok(&["index", "update"]);
    f.ok(&["index", "clear"]);
    assert_eq!(f.ok(&["status"])["recovery_required"], true);
    f.ok(&["recover", op, "--rollback"]);
    assert_eq!(f.read("a.txt"), b"old");
    assert_eq!(f.read("b.txt"), b"old");
}
