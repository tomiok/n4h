#[cfg(feature = "fault-injection")]
use super::decode;
use super::{Fixture, string};
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;

fn three_files() -> Fixture {
    let f = Fixture::new();
    for name in ["a.txt", "b.txt", "c.txt"] {
        f.write(name, format!("# existing user change in {name}\nold\n"));
    }
    f
}

#[cfg(feature = "fault-injection")]
fn pending_id(f: &Fixture) -> String {
    f.ok(&["status"])["pending_operations"][0]["operation_id"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn expect_versions(f: &Fixture, versions: [&str; 3]) {
    for (name, version) in ["a.txt", "b.txt", "c.txt"].into_iter().zip(versions) {
        assert_eq!(
            f.read(name),
            format!("# existing user change in {name}\n{version}\n").as_bytes()
        );
    }
}

#[test]
fn multifile_preview_apply_undo_and_retries_preserve_preexisting_work() {
    let f = three_files();
    let plan = f.plan("old", "new");
    assert_eq!(plan["files_total"], 3);
    assert_eq!(plan["selected_matches"], 3);
    for path in ["a.txt", "b.txt", "c.txt"] {
        assert!(string(&plan, "diff").contains(path));
    }
    expect_versions(&f, ["old", "old", "old"]);
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(applied["files_modified_this_call"], 3);
    assert_eq!(applied["files_total"], 3);
    expect_versions(&f, ["new", "new", "new"]);
    let replay = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(replay["operation_id"], applied["operation_id"]);
    assert_eq!(replay["files_modified_this_call"], 0);
    let undone = f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(undone["files_modified_this_call"], 3);
    expect_versions(&f, ["old", "old", "old"]);
    let replay = f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(replay["operation_id"], undone["operation_id"]);
    assert_eq!(replay["files_modified_this_call"], 0);
}

#[test]
fn explicit_match_ids_can_select_a_cross_file_subset() {
    let f = Fixture::new();
    f.write("a.txt", "old old old\n");
    f.write("b.txt", "old old\n");
    f.write("c.txt", "old\n");
    let found = f.find("old");
    let plan = f.ok(&[
        "edit",
        "--on",
        string(&found, "result_id"),
        "--match-id",
        "m_000002",
        "--match-id",
        "m_000004",
        "--replace",
        "new",
    ]);
    assert_eq!(plan["files_total"], 2);
    f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"old new old\n");
    assert_eq!(f.read("b.txt"), b"new old\n");
    assert_eq!(f.read("c.txt"), b"old\n");
}

#[test]
fn a_stale_file_prevents_persisting_the_entire_plan() {
    let f = three_files();
    let found = f.find("old");
    f.write("b.txt", "external old\n");
    f.error(
        &[
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new",
        ],
        "file_changed",
        3,
    );
    assert_eq!(f.table_len("plans"), 0);
    assert_eq!(f.table_len("operations"), 0);
    assert_eq!(f.read("a.txt"), b"# existing user change in a.txt\nold\n");
    assert_eq!(f.read("b.txt"), b"external old\n");
}

#[test]
fn apply_preflight_reports_all_conflicts_without_writing_other_files() {
    let f = three_files();
    let plan = f.plan("old", "new");
    f.write("b.txt", "external\n");
    fs::remove_file(f.root().join("c.txt")).unwrap();
    let error = f.error(&["apply", string(&plan, "plan_id")], "file_changed", 3);
    assert_eq!(error["error"]["paths_total"], 2);
    assert_eq!(error["error"]["paths"], json!(["b.txt", "c.txt"]));
    assert_eq!(f.table_len("operations"), 0);
    assert_eq!(f.read("a.txt"), b"# existing user change in a.txt\nold\n");
    assert_eq!(f.read("b.txt"), b"external\n");
    assert!(!f.root().join("c.txt").exists());
}

#[test]
fn undo_preflight_checks_every_file_before_restoring_any() {
    let f = three_files();
    let plan = f.plan("old", "new");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    f.write("b.txt", "external\n");
    f.error(
        &["undo", string(&applied, "checkpoint_id")],
        "file_changed",
        3,
    );
    assert_eq!(f.read("a.txt"), b"# existing user change in a.txt\nnew\n");
    assert_eq!(f.read("c.txt"), b"# existing user change in c.txt\nnew\n");
    assert_eq!(f.read("b.txt"), b"external\n");
    assert_eq!(f.table_len("operations"), 1);
}

#[test]
fn fifty_files_work_but_fifty_one_are_rejected_without_a_plan() {
    let f = Fixture::new();
    for index in 0..51 {
        f.write(&format!("f{index:02}.txt"), "old\n");
    }
    let found = f.find("old");
    f.error(
        &[
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new",
        ],
        "plan_limit_exceeded",
        5,
    );
    assert_eq!(f.table_len("plans"), 0);
    fs::remove_file(f.root().join("f50.txt")).unwrap();
    let plan = f.plan("old", "new");
    assert_eq!(plan["files_total"], 50);
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(applied["files_modified_this_call"], 50);
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    for index in 0..50 {
        assert_eq!(f.read(&format!("f{index:02}.txt")), b"old\n");
    }
}

#[test]
fn the_snapshot_byte_limit_applies_to_the_whole_plan() {
    let f = Fixture::new();
    for index in 0..6 {
        f.write(
            &format!("f{index}.txt"),
            format!("old{}", "x".repeat(1_900_000)),
        );
    }
    let found = f.find("old");
    f.error(
        &[
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new",
        ],
        "plan_limit_exceeded",
        5,
    );
    assert_eq!(f.table_len("plans"), 0);
    assert_eq!(f.table_len("operations"), 0);
}

#[test]
fn unchanged_matches_are_explicit_and_unchanged_files_are_not_written() {
    let f = Fixture::new();
    f.write("a.txt", "old new\n");
    f.write("b.txt", "new\n");
    let found = f.ok(&["find", "old|new", "--regex"]);
    let plan = f.ok(&[
        "edit",
        "--on",
        string(&found, "result_id"),
        "--scope",
        "all",
        "--replace",
        "new",
    ]);
    assert_eq!(plan["selected_matches"], 3);
    assert_eq!(plan["changed_matches"], 1);
    assert_eq!(plan["unchanged_matches"], 2);
    assert_eq!(plan["files_total"], 1);
    f.write("b.txt", "external work in the unchanged file\n");
    f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(f.read("a.txt"), b"new new\n");
    assert_eq!(f.read("b.txt"), b"external work in the unchanged file\n");
}

#[test]
fn long_path_metadata_diffs_receipts_and_observations_are_pageable() {
    let f = Fixture::new();
    let deep = (0..16)
        .map(|i| format!("{i}{}", "d".repeat(195)))
        .collect::<Vec<_>>()
        .join("/");
    let paths: Vec<_> = (0..25).map(|i| format!("{deep}/f{i:02}.txt")).collect();
    for path in &paths {
        f.write(path, "old\n");
    }
    let plan = f.plan("old", "new");
    assert_eq!(plan["files_total"], 25);
    assert_eq!(plan["files_truncated"], true);
    let id = string(&plan, "plan_id");
    let mut page = plan.clone();
    let mut seen = BTreeSet::new();
    loop {
        for file in page["files"].as_array().unwrap() {
            assert!(seen.insert(string(file, "path").to_owned()));
        }
        let Some(cursor) = page["files_cursor_next"].as_str() else {
            break;
        };
        page = f.ok(&["inspect", id, "--cursor", cursor]);
        assert_eq!(page["kind"], "plan_files");
    }
    assert_eq!(seen, paths.iter().cloned().collect());
    let mut diff = String::new();
    page = plan.clone();
    loop {
        diff.push_str(string(&page, "diff"));
        let Some(cursor) = page["cursor_next"].as_str() else {
            break;
        };
        page = f.ok(&["inspect", id, "--cursor", cursor]);
    }
    for path in &paths {
        assert!(diff.contains(&format!("--- {path}\n")));
    }
    let applied = f.ok(&["apply", id]);
    assert_eq!(applied["files_modified_this_call"], 25);
    let operation_id = string(&applied, "operation_id");
    let mut observations = f.ok(&["recover", operation_id]);
    assert_eq!(observations["truncated"], true);
    seen.clear();
    loop {
        for row in observations["observations"].as_array().unwrap() {
            assert_eq!(row["state"], "after");
            assert_eq!(row["recorded_progress"], "applied");
            assert!(seen.insert(string(row, "path").to_owned()));
        }
        let Some(cursor) = observations["cursor_next"].as_str() else {
            break;
        };
        observations = f.ok(&["recover", operation_id, "--cursor", cursor]);
    }
    assert_eq!(seen.len(), 25);
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    for path in &paths {
        assert_eq!(f.read(path), b"old\n");
    }
}

#[test]
fn duplicate_stored_paths_are_rejected_before_writing() {
    let f = three_files();
    let plan = f.plan("old", "new");
    f.rewrite_json("plans", string(&plan, "plan_id"), |p| {
        p["files"][1]["path"] = p["files"][0]["path"].clone()
    });
    f.error(&["apply", string(&plan, "plan_id")], "invalid_state", 4);
    expect_versions(&f, ["old", "old", "old"]);
    assert_eq!(f.table_len("operations"), 0);
}

#[test]
fn each_file_keeps_its_own_unicode_line_endings_and_permissions() {
    let f = Fixture::new();
    let originals = [
        ("a.txt", "\u{feff}🙂 old\r\nkeep\r\n", 0o640),
        ("b.txt", "é old old\n", 0o600),
        ("c.txt", "#!/bin/sh\nold\n", 0o755),
    ];
    for (path, text, mode) in originals {
        f.write(path, text);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(f.root().join(path), fs::Permissions::from_mode(mode)).unwrap();
        }
        let _ = mode;
    }
    let plan = f.plan("old", "nuevo🙂");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    for (path, text, mode) in originals {
        assert_eq!(f.read(path), text.replace("old", "nuevo🙂").as_bytes());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(f.root().join(path))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                mode
            );
        }
        let _ = mode;
    }
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    for (path, text, _) in originals {
        assert_eq!(f.read(path), text.as_bytes());
    }
}

// Build the exact previous redb representation: one path in each record,
// unindexed :before/:after blobs, schema 2 and no per-file progress field.
fn downgrade_fixture_to_schema_two(f: &Fixture) {
    let db = Database::open(f.root().join(".n4h/state.redb")).unwrap();
    let transaction = db.begin_write().unwrap();
    for name in ["plans", "operations"] {
        let mut table = transaction
            .open_table(TableDefinition::<&str, &str>::new(name))
            .unwrap();
        let rows: Vec<(String, Value)> = table
            .iter()
            .unwrap()
            .map(|row| {
                let (k, v) = row.unwrap();
                (
                    k.value().to_owned(),
                    serde_json::from_str(v.value()).unwrap(),
                )
            })
            .collect();
        for (key, mut value) in rows {
            if name == "operations" {
                value.as_object_mut().unwrap().remove("progress");
            }
            let target = if name == "operations" {
                &mut value["operation"]
            } else {
                &mut value
            };
            let object = target.as_object_mut().unwrap();
            let files = object.remove("files").unwrap();
            assert_eq!(files.as_array().unwrap().len(), 1);
            object.remove("snapshot_version");
            object.remove("unchanged_matches");
            object.extend(files[0].as_object().unwrap().clone());
            table
                .insert(
                    key.as_str(),
                    serde_json::to_string(&value).unwrap().as_str(),
                )
                .unwrap();
        }
    }
    {
        let mut table = transaction
            .open_table(TableDefinition::<&str, &[u8]>::new("snapshots"))
            .unwrap();
        let rows: Vec<(String, Vec<u8>)> = table
            .iter()
            .unwrap()
            .map(|row| {
                let (k, v) = row.unwrap();
                (k.value().to_owned(), v.value().to_vec())
            })
            .collect();
        for (key, bytes) in rows {
            let (prefix, suffix) = key.rsplit_once(":0:").unwrap();
            let old = format!("{prefix}:{suffix}");
            table.insert(old.as_str(), bytes.as_slice()).unwrap();
            table.remove(key.as_str()).unwrap();
        }
    }
    transaction
        .open_table(TableDefinition::<&str, &str>::new("meta"))
        .unwrap()
        .insert("schema", "2")
        .unwrap();
    transaction.commit().unwrap();
}

fn schema(f: &Fixture) -> String {
    let db = Database::open(f.root().join(".n4h/state.redb")).unwrap();
    let transaction = db.begin_read().unwrap();
    let table = transaction
        .open_table(TableDefinition::<&str, &str>::new("meta"))
        .unwrap();
    table.get("schema").unwrap().unwrap().value().to_owned()
}

#[test]
fn schema_two_plans_keep_ids_and_apply_without_rewriting_old_snapshots() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let plan = f.plan("old", "new");
    downgrade_fixture_to_schema_two(&f);
    assert_eq!(schema(&f), "2");
    assert_eq!(
        f.ok(&["inspect", string(&plan, "plan_id")])["plan_id"],
        plan["plan_id"]
    );
    assert_eq!(schema(&f), "2");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(schema(&f), "6");
    assert_eq!(f.read("a.txt"), b"new\n");
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.txt"), b"old\n");
    f.ok(&["inspect", string(&plan, "plan_id")]);
}

#[test]
fn schema_two_committed_checkpoints_can_be_replayed_and_undone() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let plan = f.plan("old", "new");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    downgrade_fixture_to_schema_two(&f);
    let replay = f.ok(&["apply", string(&plan, "plan_id")]);
    assert_eq!(replay["operation_id"], applied["operation_id"]);
    assert_eq!(schema(&f), "2");
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.txt"), b"old\n");
    assert_eq!(schema(&f), "6");
    f.ok(&["recover", string(&applied, "operation_id")]);
}

#[cfg(feature = "fault-injection")]
#[test]
fn an_io_failure_on_the_second_file_leaves_a_recoverable_partial_operation() {
    let f = three_files();
    let plan = f.plan("old", "new");
    let output = f
        .command()
        .args(["apply", string(&plan, "plan_id")])
        .env("N4H_TEST_FAILPOINT", "apply_io_before_file_2")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(4));
    let error = decode(&output);
    assert_eq!(error["error"]["code"], "io_error");
    expect_versions(&f, ["new", "old", "old"]);
    let id = pending_id(&f);
    assert_eq!(error["error"]["operation_id"], id);
    let inspected = f.ok(&["recover", &id]);
    assert_eq!(inspected["observations"][0]["recorded_progress"], "applied");
    assert_eq!(inspected["observations"][1]["state"], "before");
    let rollback = f.ok(&["recover", &id, "--rollback"]);
    assert_eq!(rollback["files_modified_this_call"], 1);
    expect_versions(&f, ["old", "old", "old"]);
    assert_eq!(f.ok(&["status"])["recovery_required"], false);
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_crash_after_the_second_rename_is_recovered_from_hashes_not_progress() {
    let f = three_files();
    let plan = f.plan("old", "new");
    f.crash(&["apply", string(&plan, "plan_id")], "after_rename_2");
    expect_versions(&f, ["new", "new", "old"]);
    let id = pending_id(&f);
    let inspected = f.ok(&["recover", &id]);
    assert_eq!(inspected["observations"][1]["state"], "after");
    assert_eq!(
        inspected["observations"][1]["recorded_progress"],
        "prepared"
    );
    let rollback = f.ok(&["recover", &id, "--rollback"]);
    assert_eq!(rollback["files_modified_this_call"], 2);
    expect_versions(&f, ["old", "old", "old"]);
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_recovery_preflight_conflict_does_not_restore_any_other_file() {
    let f = three_files();
    let plan = f.plan("old", "new");
    f.crash(&["apply", string(&plan, "plan_id")], "after_file_1");
    let id = pending_id(&f);
    f.write("c.txt", "external user work\n");
    f.error(&["recover", &id, "--rollback"], "recovery_conflict", 3);
    assert_eq!(f.read("a.txt"), b"# existing user change in a.txt\nnew\n");
    assert_eq!(f.read("b.txt"), b"# existing user change in b.txt\nold\n");
    assert_eq!(f.read("c.txt"), b"external user work\n");
    assert_eq!(f.ok(&["status"])["recovery_required"], true);
}

#[cfg(feature = "fault-injection")]
#[test]
fn rollback_io_failure_and_a_second_interruption_can_be_retried() {
    let f = three_files();
    let plan = f.plan("old", "new");
    f.crash(
        &["apply", string(&plan, "plan_id")],
        "before_committed_status_commit",
    );
    let id = pending_id(&f);
    let out = f
        .command()
        .args(["recover", &id, "--rollback"])
        .env("N4H_TEST_FAILPOINT", "rollback_io_before_file_2")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    expect_versions(&f, ["old", "new", "new"]);
    f.crash(&["recover", &id, "--rollback"], "rollback_after_rename_2");
    expect_versions(&f, ["old", "old", "new"]);
    let rollback = f.ok(&["recover", &id, "--rollback"]);
    assert_eq!(rollback["files_modified_this_call"], 1);
    expect_versions(&f, ["old", "old", "old"]);
    assert_eq!(
        f.ok(&["recover", &id, "--rollback"])["idempotent_replay"],
        true
    );
    assert_eq!(f.table_len("pending_operations"), 0);
}

#[cfg(feature = "fault-injection")]
#[test]
fn interrupted_multifile_undo_rolls_back_to_the_applied_versions() {
    let f = three_files();
    let plan = f.plan("old", "new");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    f.crash(
        &["undo", string(&applied, "checkpoint_id")],
        "after_rename_2",
    );
    expect_versions(&f, ["old", "old", "new"]);
    let id = pending_id(&f);
    f.ok(&["recover", &id, "--rollback"]);
    expect_versions(&f, ["new", "new", "new"]);
    assert_eq!(f.ok(&["status"])["recovery_required"], false);
}

#[cfg(feature = "fault-injection")]
#[test]
fn schema_two_pending_operations_survive_status_updates_and_rollback() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let plan = f.plan("old", "new");
    f.crash(&["apply", string(&plan, "plan_id")], "after_rename");
    downgrade_fixture_to_schema_two(&f);
    let id = pending_id(&f);
    let observation = f.ok(&["recover", &id]);
    assert_eq!(observation["observation"]["state"], "after");
    assert_eq!(
        observation["observations"][0]["recorded_progress"],
        "unknown"
    );
    assert_eq!(schema(&f), "2");
    f.ok(&["recover", &id, "--rollback"]);
    assert_eq!(f.read("a.txt"), b"old\n");
    assert_eq!(schema(&f), "6");
    assert_eq!(f.ok(&["recover", &id])["status"], "rolled_back");
    f.ok(&["recover", &id, "--rollback"]);
}
