use super::{Fixture, decode, string};
use redb::{Database, ReadableDatabase, TableDefinition};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::process::Stdio;

fn find(f: &Fixture, pattern: &str) -> Value {
    f.ok(&["ast", "find", pattern])
}

fn plan(f: &Fixture, result: &Value, template: &str) -> Value {
    f.ok(&[
        "ast",
        "edit",
        "--on",
        string(result, "result_id"),
        "--scope",
        "all",
        "--replace",
        template,
    ])
}

#[test]
fn rust_call_rewrites_preserve_distinct_arguments_and_do_not_touch_strings_comments_or_other_languages()
 {
    let f = Fixture::new();
    let a = "// old_api(pretend)\nfn main() { let s = \"old_api(also_fake)\"; old_api(a); old_api(b + 2); }\n";
    let b = "fn next() { old_api( function_call(x, y) ); }\n";
    f.write("a.rs", a);
    f.write("b.rs", b);
    f.write("c.js", "old_api(do_not_change);");
    let found = find(&f, "old_api($ARG)");
    assert_eq!(found["total_matches"], 3);
    assert_eq!(found["mode"], "ast_rust");
    assert_eq!(found["policy"]["language"], "rust");
    assert_eq!(found["policy"]["symbol_resolution"], false);
    assert_eq!(found["policy"]["line_based"], false);
    assert_eq!(found["matches"][0]["ast"]["node_kind"], "call_expression");
    let rewrite = plan(&f, &found, "new_api($ARG)");
    assert_eq!(rewrite["files_total"], 2);
    assert_eq!(rewrite["selected_matches"], 3);
    assert_eq!(f.read("a.rs"), a.as_bytes());
    assert_eq!(f.read("b.rs"), b.as_bytes());
    let applied = f.ok(&["apply", string(&rewrite, "plan_id")]);
    assert_eq!(applied["files_modified_this_call"], 2);
    assert_eq!(
        f.read("a.rs"),
        a.replace("old_api(a)", "new_api(a)")
            .replace("old_api(b + 2)", "new_api(b + 2)")
            .as_bytes()
    );
    assert_eq!(
        f.read("b.rs"),
        b"fn next() { new_api(function_call(x, y)); }\n"
    );
    assert_eq!(f.read("c.js"), b"old_api(do_not_change);");
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.rs"), a.as_bytes());
    assert_eq!(f.read("b.rs"), b.as_bytes());
    assert_eq!(
        f.ok(&["resume"])["operational_facts"]["recent_operations"]["total"],
        2
    );
}

#[test]
fn multiple_node_captures_support_empty_arguments_and_preserve_interior_comments_and_bytes() {
    let f = Fixture::new();
    let source =
        "fn main() { old_api(); old_api(x, /* keep this */ y); old_api(\n    x,\n    y\n); }\n";
    f.write("a.rs", source);
    let found = find(&f, "old_api($$$ARGS)");
    assert_eq!(found["total_matches"], 3);
    let empty = &found["matches"][0]["ast"]["captures"]["ARGS"];
    assert_eq!(empty["multiple"], true);
    assert_eq!(empty["start_byte"], empty["end_byte"]);
    let rewrite = plan(&f, &found, "new_api($$$ARGS)");
    let applied = f.ok(&["apply", string(&rewrite, "plan_id")]);
    assert_eq!(
        f.read("a.rs"),
        b"fn main() { new_api(); new_api(x, /* keep this */ y); new_api(x,\n    y); }\n"
    );
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.rs"), source.as_bytes());
}

#[test]
fn captured_ranges_persist_across_calls_and_are_verified_against_the_original_file() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x + 1); }\n");
    let found = find(&f, "old_api($ARG)");
    let inspected = f.ok(&["inspect", string(&found, "result_id")]);
    assert_eq!(inspected["matches"], found["matches"]);
    let context = f.ok(&[
        "inspect",
        string(&found, "result_id"),
        "--match-id",
        "m_000001",
    ]);
    assert_eq!(
        context["match"]["ast"]["captures"]["ARG"],
        found["matches"][0]["ast"]["captures"]["ARG"]
    );
    f.write("a.rs", "fn main() { old_api(external); }\n");
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new_api($ARG)",
        ],
        "file_changed",
        3,
    );
    assert_eq!(f.table_len("plans"), 0);
    assert_eq!(f.read("a.rs"), b"fn main() { old_api(external); }\n");
}

#[test]
fn literal_and_ast_result_types_cannot_be_silently_mixed() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    let text = f.find("old_api");
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&text, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new_api",
        ],
        "ast_result_required",
        2,
    );
    let structural = find(&f, "old_api($ARG)");
    f.error(
        &[
            "edit",
            "--on",
            string(&structural, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new_api($ARG)",
        ],
        "ast_edit_required",
        2,
    );
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&structural, "result_id"),
            "--replace",
            "new_api($ARG)",
        ],
        "scope_required",
        2,
    );
    assert_eq!(f.table_len("plans"), 0);
    // The ordinary text-edit contract remains literal, including dollar sequences.
    let text = f.find("old_api");
    let replacement = f.ok(&[
        "edit",
        "--on",
        string(&text, "result_id"),
        "--scope",
        "all",
        "--replace",
        "$1",
    ]);
    f.ok(&["apply", string(&replacement, "plan_id")]);
    assert_eq!(f.read("a.rs"), b"fn main() { $1(x); }\n");
}

#[test]
fn invalid_templates_unknown_captures_and_invalid_rust_output_never_save_a_plan() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    let found = find(&f, "old_api($ARG)");
    for (replacement, code, exit) in [
        ("new_api($MISSING)", "unknown_capture", 2),
        ("new_api($$$ARG)", "invalid_arguments", 2),
        ("new_api($1)", "invalid_arguments", 2),
        ("new_api($arg)", "invalid_arguments", 2),
        ("new_api($ARG", "ast_rewrite_invalid", 3),
        ("let =", "ast_rewrite_invalid", 3),
    ] {
        f.error(
            &[
                "ast",
                "edit",
                "--on",
                string(&found, "result_id"),
                "--scope",
                "all",
                "--replace",
                replacement,
            ],
            code,
            exit,
        );
    }
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            &"x".repeat(65537),
        ],
        "ast_limit_exceeded",
        5,
    );
    assert_eq!(f.table_len("plans"), 0);
    assert_eq!(f.read("a.rs"), b"fn main() { old_api(x); }\n");
}

#[test]
fn malformed_sources_make_coverage_incomplete_and_require_a_narrower_search() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    f.write("broken.rs", "fn broken( { old_api(x)\n");
    let found = find(&f, "old_api($ARG)");
    assert_eq!(found["scan_complete"], true);
    assert_eq!(found["coverage_complete"], false);
    assert_eq!(found["total_matches"], 1);
    assert!(
        found["skipped"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["reason"] == "rust_parse_error")
    );
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new_api($ARG)",
        ],
        "incomplete_coverage",
        3,
    );
    let narrowed = f.ok(&["ast", "find", "old_api($ARG)", "a.rs"]);
    assert_eq!(narrowed["coverage_complete"], true);
    plan(&f, &narrowed, "new_api($ARG)");
}

#[test]
fn malformed_or_oversized_patterns_do_not_create_handles() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    for pattern in [
        "",
        " ",
        "old_api(",
        "fn {",
        "old_api($A); old_api($B);",
        "$$$ARGS",
    ] {
        f.error(&["ast", "find", pattern], "invalid_arguments", 2);
    }
    f.error(&["ast", "find", &"x".repeat(16385)], "invalid_arguments", 2);
    let vars = (0..33)
        .map(|i| format!("$A{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    f.error(
        &["ast", "find", &format!("old_api({vars})")],
        "invalid_arguments",
        2,
    );
    assert_eq!(f.table_len("results"), 0);
}

#[test]
fn nested_matches_require_explicit_nonoverlapping_selection() {
    let f = Fixture::new();
    let source = "fn main() { old_api(old_api(x)); }\n";
    f.write("a.rs", source);
    let found = find(&f, "old_api($ARG)");
    assert_eq!(found["total_matches"], 2);
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new_api($ARG)",
        ],
        "overlapping_matches",
        3,
    );
    assert_eq!(f.table_len("plans"), 0);
    let outer = f.ok(&[
        "ast",
        "edit",
        "--on",
        string(&found, "result_id"),
        "--match-id",
        "m_000001",
        "--replace",
        "new_api($ARG)",
    ]);
    let applied = f.ok(&["apply", string(&outer, "plan_id")]);
    assert_eq!(f.read("a.rs"), b"fn main() { new_api(old_api(x)); }\n");
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("a.rs"), source.as_bytes());
}

#[test]
fn repeated_capture_names_enforce_the_same_syntax_without_resolving_symbols() {
    let f = Fixture::new();
    f.write(
        "a.rs",
        "fn main() { check(x, x); check(x, y); } fn other() { check(x, x); }\n",
    );
    let found = find(&f, "check($A, $A)");
    assert_eq!(found["total_matches"], 2);
    let rewrite = plan(&f, &found, "checked($A)");
    f.ok(&["apply", string(&rewrite, "plan_id")]);
    assert_eq!(
        f.read("a.rs"),
        b"fn main() { checked(x); check(x, y); } fn other() { checked(x); }\n"
    );
}

#[test]
fn item_and_statement_patterns_can_rewrite_syntax_without_renaming_references() {
    let f = Fixture::new();
    f.write("a.rs", "fn old() { work(); }\nfn main() { old(); }\n");
    let found = find(&f, "fn old() { $$$BODY }");
    assert_eq!(found["total_matches"], 1);
    let rewrite = plan(&f, &found, "fn new() { $$$BODY }");
    f.ok(&["apply", string(&rewrite, "plan_id")]);
    // Deliberately unresolved old(): AST rewriting does not follow references.
    assert_eq!(
        f.read("a.rs"),
        b"fn new() { work(); }\nfn main() { old(); }\n"
    );
    f.write("b.rs", "fn example() { let x = 1 + 2; }\n");
    let found = f.ok(&["ast", "find", "let $NAME = $VALUE;", "b.rs"]);
    assert_eq!(found["total_matches"], 1);
    let rewrite = plan(&f, &found, "let mut $NAME = $VALUE;");
    f.ok(&["apply", string(&rewrite, "plan_id")]);
    assert_eq!(f.read("b.rs"), b"fn example() { let mut x = 1 + 2; }\n");
}

#[test]
fn line_column_and_capture_ranges_use_bytes_with_unicode_bom_crlf() {
    let f = Fixture::new();
    let source = "\u{feff}// 🙂\r\nfn main() { let café = 1; old_api(café); }\r\n";
    f.write("日本語.rs", source);
    let found = find(&f, "old_api($ARG)");
    assert_eq!(found["total_matches"], 1);
    let offset = source.find("old_api").unwrap();
    assert_eq!(found["matches"][0]["start_byte"], offset);
    assert_eq!(found["matches"][0]["line"], 2);
    assert_eq!(
        found["matches"][0]["column_byte"],
        offset - source.find('\n').unwrap()
    );
    let rewrite = plan(&f, &found, "new_api($ARG)");
    let applied = f.ok(&["apply", string(&rewrite, "plan_id")]);
    assert_eq!(
        f.read("日本語.rs"),
        source.replace("old_api", "new_api").as_bytes()
    );
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(f.read("日本語.rs"), source.as_bytes());
}

#[test]
fn matches_page_without_repeating_the_query_and_captures_remain_intact() {
    let f = Fixture::new();
    f.write(
        "a.rs",
        format!("fn main() {{ {} }}\n", "old_api(x); ".repeat(170)),
    );
    let mut page = f.ok(&["ast", "find", "old_api($ARG)", "--page-size", "2"]);
    let id = string(&page, "result_id").to_owned();
    f.write("a.rs", "fn external() {}\n");
    let mut seen = BTreeSet::new();
    loop {
        assert_eq!(page["total_matches"], 170);
        for row in page["matches"].as_array().unwrap() {
            assert_eq!(row["ast"]["captures"]["ARG"]["multiple"], false);
            assert!(seen.insert(string(row, "match_id").to_owned()));
        }
        let Some(cursor) = page["cursor_next"].as_str() else {
            break;
        };
        page = f.ok(&["inspect", &id, "--cursor", cursor, "--page-size", "100"]);
    }
    assert_eq!(seen.len(), 170);
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            &id,
            "--scope",
            "all",
            "--replace",
            "new_api($ARG)",
        ],
        "file_changed",
        3,
    );
}

#[test]
fn template_files_stdin_literal_dollars_and_noop_are_explicit() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    let found = find(&f, "old_api($ARG)");
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "old_api($ARG)",
        ],
        "no_changes",
        3,
    );
    let file = tempfile::NamedTempFile::new().unwrap();
    fs::write(file.path(), "new_api($ARG, \"$$HOME\")").unwrap();
    let rewrite = f.ok(&[
        "ast",
        "edit",
        "--on",
        string(&found, "result_id"),
        "--scope",
        "all",
        "--replacement-file",
        file.path().to_str().unwrap(),
    ]);
    fs::write(file.path(), "invalid after preview").unwrap();
    let applied = f.ok(&["apply", string(&rewrite, "plan_id")]);
    assert_eq!(f.read("a.rs"), b"fn main() { new_api(x, \"$HOME\"); }\n");
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    let mut child = f
        .command()
        .args([
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
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
        .write_all(b"new_api(\n$ARG\n)")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    let rewrite = decode(&output);
    f.ok(&["apply", string(&rewrite, "plan_id")]);
    assert_eq!(f.read("a.rs"), b"fn main() { new_api(\nx\n); }\n");
}

#[test]
fn a_syntax_error_in_any_rewritten_file_prevents_saving_the_whole_plan() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    f.write("b.rs", "fn main() { old_api(\"hello\"); }\n");
    let found = find(&f, "old_api($ARG)");
    // For x this is valid x::method; for a string literal it is invalid syntax.
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "$ARG::method()",
        ],
        "ast_rewrite_invalid",
        3,
    );
    assert_eq!(f.table_len("plans"), 0);
    assert_eq!(f.table_len("operations"), 0);
    assert_eq!(f.read("a.rs"), b"fn main() { old_api(x); }\n");
}

#[test]
fn capture_expansion_honors_the_aggregate_byte_limit_before_allocating_large_outputs() {
    let f = Fixture::new();
    f.write(
        "a.rs",
        format!("fn main() {{ old_api(\"{}\"); }}\n", "x".repeat(900_000)),
    );
    let found = find(&f, "old_api($ARG)");
    let replacement = format!("new_api({})", vec!["$ARG"; 30].join(","));
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            &replacement,
        ],
        "plan_limit_exceeded",
        5,
    );
    assert_eq!(f.table_len("plans"), 0);
}

#[test]
fn externally_changed_preview_and_undo_keep_the_normal_conflict_protection() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    f.write("b.rs", "fn main() { old_api(y); }\n");
    let found = find(&f, "old_api($ARG)");
    let rewrite = plan(&f, &found, "new_api($ARG)");
    f.write("b.rs", "fn external() {}\n");
    f.error(&["apply", string(&rewrite, "plan_id")], "file_changed", 3);
    assert_eq!(f.read("a.rs"), b"fn main() { old_api(x); }\n");
    assert_eq!(f.table_len("operations"), 0);
    f.write("b.rs", "fn main() { old_api(y); }\n");
    let applied = f.ok(&["apply", string(&rewrite, "plan_id")]);
    f.write("b.rs", "fn external() {}\n");
    f.error(
        &["undo", string(&applied, "checkpoint_id")],
        "file_changed",
        3,
    );
    assert_eq!(f.read("a.rs"), b"fn main() { new_api(x); }\n");
}

#[test]
fn older_result_records_without_ast_fields_still_support_literal_edits() {
    let f = Fixture::new();
    f.write("a.rs", "fn old() {}\n");
    let found = f.find("old");
    f.rewrite_json("results", string(&found, "result_id"), |r| {
        for m in r["matches"].as_array_mut().unwrap() {
            m.as_object_mut().unwrap().remove("ast");
        }
    });
    let rewrite = f.ok(&[
        "edit",
        "--on",
        string(&found, "result_id"),
        "--scope",
        "all",
        "--replace",
        "new",
    ]);
    f.ok(&["apply", string(&rewrite, "plan_id")]);
    assert_eq!(f.read("a.rs"), b"fn new() {}\n");
}

#[test]
fn schema_four_memory_and_checkpoints_are_preserved_when_ast_state_is_added() {
    let f = Fixture::new();
    f.ok(&["mem", "put", "project.tests", "--text", "cargo test"]);
    f.write("a.rs", "fn main() { old_api(x); }\n");
    {
        let db = Database::open(f.root().join(".n4h/state.redb")).unwrap();
        let transaction = db.begin_write().unwrap();
        transaction
            .open_table(TableDefinition::<&str, &str>::new("meta"))
            .unwrap()
            .insert("schema", "4")
            .unwrap();
        transaction.commit().unwrap();
    }
    f.ok(&["resume"]);
    let found = find(&f, "old_api($ARG)");
    {
        let db = Database::open(f.root().join(".n4h/state.redb")).unwrap();
        let transaction = db.begin_read().unwrap();
        let table = transaction
            .open_table(TableDefinition::<&str, &str>::new("meta"))
            .unwrap();
        assert_eq!(table.get("schema").unwrap().unwrap().value(), "6");
    }
    let rewrite = plan(&f, &found, "new_api($ARG)");
    let applied = f.ok(&["apply", string(&rewrite, "plan_id")]);
    f.ok(&["undo", string(&applied, "checkpoint_id")]);
    assert_eq!(
        f.ok(&["mem", "get", "project.tests"])["note"]["text"],
        "cargo test"
    );
}

#[test]
fn corrupted_capture_ranges_do_not_create_a_plan() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    let found = find(&f, "old_api($ARG)");
    f.rewrite_json("results", string(&found, "result_id"), |r| {
        r["matches"][0]["ast"]["captures"]["ARG"]["end_byte"] = json!(9999)
    });
    f.error(
        &[
            "ast",
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new_api($ARG)",
        ],
        "invalid_state",
        4,
    );
    assert_eq!(f.table_len("plans"), 0);
}

#[cfg(feature = "fault-injection")]
#[test]
fn an_interrupted_ast_rewrite_uses_the_same_multifile_recovery_journal() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); }\n");
    f.write("b.rs", "fn main() { old_api(y); }\n");
    let found = find(&f, "old_api($ARG)");
    let rewrite = plan(&f, &found, "new_api($ARG)");
    f.crash(&["apply", string(&rewrite, "plan_id")], "after_rename_1");
    assert_eq!(f.read("a.rs"), b"fn main() { new_api(x); }\n");
    assert_eq!(f.read("b.rs"), b"fn main() { old_api(y); }\n");
    let pending = f.ok(&["status"]);
    let op = pending["pending_operations"][0]["operation_id"]
        .as_str()
        .unwrap();
    f.ok(&["recover", op, "--rollback"]);
    assert_eq!(f.read("a.rs"), b"fn main() { old_api(x); }\n");
    assert_eq!(f.read("b.rs"), b"fn main() { old_api(y); }\n");
}
