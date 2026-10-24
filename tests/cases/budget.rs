use super::{Fixture, decode, string};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use tiktoken_rs::CoreBPE;

fn encoder(name: &str) -> &'static CoreBPE {
    match name {
        "o200k_base" => tiktoken_rs::o200k_base_singleton(),
        "cl100k_base" => tiktoken_rs::cl100k_base_singleton(),
        _ => panic!("unsupported test tokenizer"),
    }
}

fn budgeted(f: &Fixture, args: &[&str], limit: u32, tokenizer: &str, exit: i32) -> Value {
    let mut arguments = args.to_vec();
    let limit_text = limit.to_string();
    arguments.extend(["--max-tokens", &limit_text, "--tokenizer", tokenizer]);
    let output = f.run(&arguments);
    assert_eq!(
        output.status.code(),
        Some(exit),
        "{arguments:?}: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value = decode(&output);
    let serialized = std::str::from_utf8(&output.stdout).unwrap();
    assert!(serialized.ends_with('\n'));
    let actual = encoder(tokenizer).encode_ordinary(serialized).len();
    assert!(
        actual <= limit as usize,
        "{actual} tokens > {limit}: {value}"
    );
    assert_eq!(value["budget"]["output_tokens"], actual);
    assert_eq!(value["budget"]["max_tokens"], limit);
    assert_eq!(value["budget"]["tokenizer"], tokenizer);
    assert_eq!(
        value["budget"]["count_scope"],
        "serialized_json_plus_newline"
    );
    assert!(output.stdout.len() <= 65536);
    value
}

#[test]
fn flags_require_an_explicit_supported_pair_and_a_valid_minimum() {
    let f = Fixture::empty();
    for args in [
        vec!["init", "--max-tokens", "1000"],
        vec!["init", "--tokenizer", "o200k_base"],
        vec!["init", "--max-tokens", "255", "--tokenizer", "o200k_base"],
        vec![
            "init",
            "--max-tokens",
            "1000001",
            "--tokenizer",
            "o200k_base",
        ],
        vec![
            "init",
            "--max-tokens",
            "1000",
            "--tokenizer",
            "guess-my-model",
        ],
    ] {
        f.error(&args, "invalid_arguments", 2);
    }
    assert!(!f.root().join(".n4h").exists());
    let initialized = budgeted(&f, &["init"], 1500, "o200k_base", 0);
    assert_eq!(initialized["kind"], "initialized");
    let value = f.ok(&[
        "--max-tokens",
        "1000",
        "--tokenizer",
        "cl100k_base",
        "status",
    ]);
    assert_eq!(value["budget"]["tokenizer"], "cl100k_base");
    assert!(
        !f.ok(&["status"])
            .as_object()
            .unwrap()
            .contains_key("budget")
    );
}

#[test]
fn token_counts_include_escaping_metadata_unicode_and_special_token_like_text() {
    let f = Fixture::new();
    let text = "日本語🙂\n\"quoted\" \\ escaped\t\u{1} <|endoftext|> <|fim_prefix|>".repeat(8);
    f.ok(&["mem", "put", "test", "--text", &text]);
    for tokenizer in ["o200k_base", "cl100k_base"] {
        let note = budgeted(&f, &["mem", "get", "test"], 6000, tokenizer, 0);
        assert_eq!(note["note"]["text"], text);
        assert_eq!(note["budget"]["special_tokens"], "ordinary_text");
        let page = budgeted(&f, &["mem", "search", "日本語"], 6000, tokenizer, 0);
        assert_eq!(page["total_matches"], 1);
    }
}

#[test]
fn search_pages_retain_all_matches_and_do_not_narrow_edit_scope() {
    let f = Fixture::new();
    f.write("a.txt", "old 日本語🙂\n".repeat(20));
    let mut page = budgeted(
        &f,
        &["find", "old", "--page-size", "500"],
        1000,
        "o200k_base",
        0,
    );
    assert_eq!(page["total_matches"], 20);
    assert!(page["shown_matches"].as_u64().unwrap() < 20);
    let id = string(&page, "result_id").to_owned();
    let mut seen = BTreeSet::new();
    loop {
        for m in page["matches"].as_array().unwrap() {
            assert!(seen.insert(string(m, "match_id").to_owned()));
        }
        let Some(cursor) = page["cursor_next"].as_str() else {
            break;
        };
        page = budgeted(
            &f,
            &["inspect", &id, "--cursor", cursor, "--page-size", "500"],
            1000,
            "cl100k_base",
            0,
        );
    }
    assert_eq!(seen.len(), 20);
    let plan = budgeted(
        &f,
        &["edit", "--on", &id, "--scope", "all", "--replace", "new"],
        2000,
        "o200k_base",
        0,
    );
    assert_eq!(plan["selected_matches"], 20);
    let applied = budgeted(
        &f,
        &["apply", string(&plan, "plan_id")],
        1000,
        "o200k_base",
        0,
    );
    assert_eq!(f.read("a.txt"), "new 日本語🙂\n".repeat(20).as_bytes());
    let replay = budgeted(
        &f,
        &["apply", string(&plan, "plan_id")],
        1000,
        "cl100k_base",
        0,
    );
    assert_eq!(replay["operation_id"], applied["operation_id"]);
    budgeted(
        &f,
        &["undo", string(&applied, "checkpoint_id")],
        1000,
        "cl100k_base",
        0,
    );
    assert_eq!(f.read("a.txt"), "old 日本語🙂\n".repeat(20).as_bytes());
}

#[test]
fn indexed_and_ast_search_keep_their_scope_and_capture_metadata_under_budget() {
    let f = Fixture::new();
    f.write("a.rs", "fn main() { old_api(x); old_api(y); }\n");
    let built = budgeted(&f, &["index", "build"], 512, "o200k_base", 0);
    assert_eq!(built["ready"], true);
    let indexed = budgeted(
        &f,
        &["find", "old_api", "--indexed"],
        1500,
        "cl100k_base",
        0,
    );
    assert_eq!(indexed["scope"], "indexed_snapshot");
    assert_eq!(indexed["index_generation"], 1);
    assert!(!string(&indexed, "snapshot_warning").is_empty());
    let ast = budgeted(&f, &["ast", "find", "old_api($ARG)"], 1500, "o200k_base", 0);
    assert_eq!(ast["total_matches"], 2);
    assert!(ast["matches"][0]["ast"]["captures"]["ARG"].is_object());
    let rewrite = budgeted(
        &f,
        &[
            "ast",
            "edit",
            "--on",
            string(&ast, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new_api($ARG)",
        ],
        2000,
        "o200k_base",
        0,
    );
    budgeted(
        &f,
        &["apply", string(&rewrite, "plan_id")],
        1000,
        "o200k_base",
        0,
    );
    assert_eq!(f.read("a.rs"), b"fn main() { new_api(x); new_api(y); }\n");
}

#[test]
fn diff_token_pages_end_at_line_boundaries_and_reconstruct_the_exact_original_diff() {
    let f = Fixture::new();
    let source = (0..40)
        .map(|i| format!("line{i:03}_old 日本語🙂🙂🙂🙂🙂\n"))
        .collect::<String>();
    f.write("a.txt", &source);
    let plan = f.plan("old", "new");
    assert_eq!(plan["diff_truncated"], false);
    let expected = string(&plan, "diff");
    let mut page = budgeted(
        &f,
        &["inspect", string(&plan, "plan_id")],
        1000,
        "o200k_base",
        0,
    );
    let mut combined = String::new();
    let mut pages = 0;
    loop {
        assert_eq!(page["diff_chunk_boundary"], "line_end");
        let chunk = string(&page, "diff");
        assert!(chunk.ends_with('\n'));
        assert_eq!(page["diff_start_byte"], combined.len());
        combined.push_str(chunk);
        pages += 1;
        let Some(cursor) = page["cursor_next"].as_str() else {
            break;
        };
        assert!(pages < 100);
        page = budgeted(
            &f,
            &["inspect", string(&plan, "plan_id"), "--cursor", cursor],
            1200,
            "cl100k_base",
            0,
        );
    }
    assert!(pages > 1);
    assert_eq!(combined, expected);
    assert_eq!(f.read("a.txt"), source.as_bytes());
}

#[test]
fn plan_file_metadata_has_its_own_complete_token_bounded_pagination() {
    let f = Fixture::new();
    for i in 0..10 {
        f.write(&format!("file{i:02}.txt"), "old\n");
    }
    let plan = f.plan("old", "new");
    let mut page = budgeted(
        &f,
        &["inspect", string(&plan, "plan_id")],
        1000,
        "o200k_base",
        0,
    );
    assert_eq!(page["files_total"], 10);
    assert_eq!(page["selected_matches"], 10);
    assert_eq!(page["files_truncated"], true);
    let mut seen = BTreeSet::new();
    loop {
        for file in page["files"].as_array().unwrap() {
            assert!(seen.insert(string(file, "path").to_owned()));
        }
        let Some(cursor) = page["files_cursor_next"].as_str() else {
            break;
        };
        page = budgeted(
            &f,
            &["inspect", string(&plan, "plan_id"), "--cursor", cursor],
            1000,
            "cl100k_base",
            0,
        );
        assert_eq!(page["kind"], "plan_files");
        assert_eq!(page["files_total"], 10);
    }
    assert_eq!(seen.len(), 10);
    assert_eq!(f.table_len("operations"), 0);
}

#[test]
fn full_notes_are_paged_without_truncation_and_one_oversized_note_fails_explicitly() {
    let f = Fixture::new();
    for i in 0..4 {
        f.ok(&[
            "mem",
            "put",
            &format!("n{i}"),
            "--text",
            &"日本語🙂\"\\\n".repeat(30),
        ]);
    }
    let mut page = budgeted(
        &f,
        &["mem", "list", "--page-size", "100"],
        1500,
        "o200k_base",
        0,
    );
    let mut seen = BTreeSet::new();
    loop {
        for note in page["notes"].as_array().unwrap() {
            assert_eq!(note["text"], "日本語🙂\"\\\n".repeat(30));
            assert!(seen.insert(string(note, "key").to_owned()));
        }
        let Some(cursor) = page["cursor_next"].as_str() else {
            break;
        };
        page = budgeted(
            &f,
            &["mem", "list", "--cursor", cursor],
            1500,
            "cl100k_base",
            0,
        );
    }
    assert_eq!(seen.len(), 4);
    f.ok(&["mem", "put", "huge", "--text", &"\u{1}".repeat(8192)]);
    let error = budgeted(&f, &["mem", "get", "huge"], 256, "o200k_base", 5);
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert_eq!(
        f.ok(&["mem", "get", "huge"])["note"]["text"],
        "\u{1}".repeat(8192)
    );
}

#[test]
fn the_byte_limit_still_applies_even_with_a_very_large_token_allowance() {
    let f = Fixture::new();
    for i in 0..4 {
        f.ok(&[
            "mem",
            "put",
            &format!("n{i}"),
            "--text",
            &"\u{1}".repeat(8192),
        ]);
    }
    let value = budgeted(
        &f,
        &["mem", "list", "--page-size", "100"],
        1_000_000,
        "cl100k_base",
        0,
    );
    assert_eq!(value["shown_notes"], 1);
    assert_eq!(value["truncated"], true);
    assert_eq!(value["budget"]["max_bytes"], 65536);
}

#[test]
fn context_reduces_whole_lines_without_dropping_the_matched_line() {
    let f = Fixture::new();
    let source = (0..21)
        .map(|i| {
            format!(
                "{} {}\n",
                if i == 10 { "needle" } else { "other" },
                "日本語🙂".repeat(30)
            )
        })
        .collect::<String>();
    f.write("a.txt", source);
    let found = f.find("needle");
    let context = budgeted(
        &f,
        &[
            "inspect",
            string(&found, "result_id"),
            "--match-id",
            "m_000001",
            "--context",
            "10",
        ],
        1200,
        "o200k_base",
        0,
    );
    assert_eq!(context["context_truncated"], true);
    assert!(
        context["context"]
            .as_array()
            .unwrap()
            .iter()
            .any(|l| l["line"] == 11)
    );
    assert!(context["context"].as_array().unwrap().len() < 21);
}

#[test]
fn a_budget_failure_creates_no_unreviewable_result_plan_or_note() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    let error = budgeted(&f, &["find", "old"], 256, "o200k_base", 5);
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert_eq!(f.table_len("results"), 0);
    let found = f.find("old");
    let error = budgeted(
        &f,
        &[
            "edit",
            "--on",
            string(&found, "result_id"),
            "--scope",
            "all",
            "--replace",
            "new",
        ],
        256,
        "cl100k_base",
        5,
    );
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert_eq!(f.table_len("plans"), 0);
    let first = f.ok(&["mem", "put", "existing", "--text", "original"]);
    let error = budgeted(
        &f,
        &["mem", "put", "new.note", "--text", &"🙂".repeat(1000)],
        256,
        "o200k_base",
        5,
    );
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    let error = budgeted(
        &f,
        &[
            "mem",
            "put",
            "existing",
            "--text",
            &"🙂".repeat(1000),
            "--if-revision",
            "1",
        ],
        256,
        "cl100k_base",
        5,
    );
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert_eq!(f.ok(&["mem", "get", "existing"])["note"], first["note"]);
    assert_eq!(f.table_len("notes"), 1);
    assert_eq!(f.read("a.txt"), b"old");
}

fn long_path() -> String {
    (0..9)
        .map(|i| format!("{i}_{}", "abcdef0123456789".repeat(11)))
        .collect::<Vec<_>>()
        .join("/")
        + "/a.txt"
}

#[test]
fn apply_and_undo_refuse_an_unreportable_receipt_before_journaling_or_writing() {
    let f = Fixture::new();
    let path = long_path();
    f.write(&path, "old");
    let plan = f.plan("old", "new");
    let error = budgeted(
        &f,
        &["apply", string(&plan, "plan_id")],
        256,
        "o200k_base",
        5,
    );
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert_eq!(f.table_len("operations"), 0);
    assert_eq!(f.read(&path), b"old");
    let applied = f.ok(&["apply", string(&plan, "plan_id")]);
    let error = budgeted(
        &f,
        &["undo", string(&applied, "checkpoint_id")],
        256,
        "cl100k_base",
        5,
    );
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert_eq!(f.table_len("operations"), 1);
    assert_eq!(f.read(&path), b"new");
    budgeted(
        &f,
        &["undo", string(&applied, "checkpoint_id")],
        10000,
        "o200k_base",
        0,
    );
    assert_eq!(f.read(&path), b"old");
}

#[test]
fn init_and_cache_clear_also_check_their_receipt_before_mutating() {
    let outer = tempfile::tempdir().unwrap();
    let parent = outer
        .path()
        .join(long_path())
        .parent()
        .unwrap()
        .to_path_buf();
    fs::create_dir_all(&parent).unwrap();
    let f = Fixture {
        dir: tempfile::tempdir_in(parent).unwrap(),
    };
    let error = budgeted(&f, &["init"], 256, "cl100k_base", 5);
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert!(!f.root().join(".n4h").exists());
    f.ok(&["init"]);
    f.write("a.txt", "old");
    let built = budgeted(&f, &["index", "build"], 512, "o200k_base", 0);
    assert_eq!(built["ready"], true);
    assert_eq!(built["details_truncated"], true);
    let error = budgeted(&f, &["index", "clear"], 256, "o200k_base", 5);
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert!(f.root().join(".n4h/index.redb").exists());
    assert_eq!(
        budgeted(&f, &["index", "status"], 512, "cl100k_base", 0)["ready"],
        true
    );
    budgeted(&f, &["index", "clear"], 10000, "o200k_base", 0);
    assert!(!f.root().join(".n4h/index.redb").exists());
}

#[test]
fn oversized_diff_lines_fail_instead_of_returning_a_nonadvancing_cursor_or_partial_line() {
    let f = Fixture::new();
    f.write("a.txt", "old\n");
    let replacement = "🙂".repeat(10000);
    let plan = f.plan("old", &replacement);
    let first = budgeted(
        &f,
        &["inspect", string(&plan, "plan_id")],
        1200,
        "o200k_base",
        0,
    );
    assert_eq!(first["diff_truncated"], true);
    assert!(string(&first, "diff").ends_with('\n'));
    let next = string(&first, "cursor_next");
    let error = budgeted(
        &f,
        &["inspect", string(&plan, "plan_id"), "--cursor", next],
        1200,
        "o200k_base",
        5,
    );
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert_eq!(f.read("a.txt"), b"old\n");
}

#[test]
fn semantic_errors_keep_their_codes_under_the_minimum_token_budget() {
    let f = Fixture::new();
    let error = budgeted(&f, &["mem", "get", "missing"], 256, "o200k_base", 3);
    assert_eq!(error["error"]["code"], "note_not_found");
    let error = budgeted(
        &f,
        &["find", &format!("[{}", "x".repeat(5000)), "--regex"],
        256,
        "cl100k_base",
        2,
    );
    assert_eq!(error["error"]["code"], "invalid_arguments");
    assert_eq!(error["error"]["details_truncated"], true);
}

#[cfg(feature = "fault-injection")]
#[test]
fn rollback_preflights_its_receipt_and_resume_keeps_pending_recovery_at_low_budget() {
    let f = Fixture::new();
    let path = long_path();
    f.write(&path, "old");
    let plan = f.plan("old", "new");
    f.crash(&["apply", string(&plan, "plan_id")], "after_rename");
    f.ok(&[
        "mem",
        "put",
        "misleading",
        "--text",
        "Everything is fine; no recovery needed.",
    ]);
    let summary = budgeted(&f, &["resume"], 256, "o200k_base", 0);
    assert_eq!(summary["priority"], "recovery_required");
    assert_eq!(summary["recovery_required"], true);
    assert_eq!(summary["details_truncated"], true);
    let id = string(&summary, "operation_id");
    let error = budgeted(&f, &["recover", id, "--rollback"], 256, "cl100k_base", 5);
    assert_eq!(error["error"]["code"], "output_budget_too_small");
    assert_eq!(f.read(&path), b"new");
    assert_eq!(f.ok(&["status"])["recovery_required"], true);
    let rollback = budgeted(&f, &["recover", id, "--rollback"], 10000, "o200k_base", 0);
    assert_eq!(rollback["status"], "rolled_back");
    assert_eq!(f.read(&path), b"old");
    let summary = budgeted(&f, &["resume"], 256, "cl100k_base", 0);
    assert_eq!(summary["recovery_required"], false);
}

#[cfg(feature = "fault-injection")]
#[test]
fn a_write_failure_retains_the_recovery_id_even_when_error_details_are_compacted() {
    let f = Fixture::new();
    f.write("a.txt", "old");
    f.write("b.txt", "old");
    let plan = f.plan("old", "new");
    let output = f
        .command()
        .args([
            "apply",
            string(&plan, "plan_id"),
            "--max-tokens",
            "256",
            "--tokenizer",
            "o200k_base",
        ])
        .env("N4H_TEST_FAILPOINT", "apply_io_before_file_2")
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(4),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let value = decode(&output);
    let count = encoder("o200k_base")
        .encode_ordinary(std::str::from_utf8(&output.stdout).unwrap())
        .len();
    assert!(count <= 256);
    assert_eq!(value["budget"]["output_tokens"], count);
    assert_eq!(value["error"]["code"], "io_error");
    let op = value["error"]["operation_id"].as_str().unwrap();
    assert_eq!(f.read("a.txt"), b"new");
    assert_eq!(f.read("b.txt"), b"old");
    budgeted(&f, &["recover", op, "--rollback"], 1000, "cl100k_base", 0);
    assert_eq!(f.read("a.txt"), b"old");
}
