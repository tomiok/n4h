# n4h — Not for Human

Persistent search references and verifiable text edits for coding agents.

**Search once. Review an immutable plan. Apply exactly those bytes. Refuse conflicts. Undo without discarding someone else's work.**

This is an initial Rust MVP, not a semantic refactoring engine. The verified target is Linux. Each plan and operation can edit **up to 50 regular UTF-8 files**, with multiple selected matches per file. Multi-file edits are recoverable, **not atomic across the filesystem**.

## Installation and harness setup

**Publication status:** the installer and release pipeline are prepared in this
change. The public command below requires publishing `install.sh` to `main` and
publishing the first release with the matching archive and `SHA256SUMS`.
Until then, use the source build below. No release is published merely by adding
these files; the workflow creates a **draft** for maintainer review.

Once published, on **Linux x86-64 with GNU libc >= 2.35**:

```sh
curl -fsSL https://raw.githubusercontent.com/tomiok/n4h/main/install.sh | bash
```

The installer previews the binary destination (`~/.local/bin/n4h`), asks for
confirmation, downloads a release, verifies its SHA-256 checksum, and runs
`n4h setup`. Choose one or several harnesses, then user-wide or current-project
scope. Choose `--cli-only` to skip skills. Menus read `/dev/tty`, not the stdin
occupied by a piped script. Bash, curl, tar, sha256sum and standard GNU/Linux
utilities are required; prebuilt installation does not require Rust or Git.
macOS, Windows, ARM and musl release artifacts are not provided yet.

For unattended installation, explicitly select the harnesses/scope:

```sh
installer=$(mktemp)
curl -fsSL https://raw.githubusercontent.com/tomiok/n4h/main/install.sh \
  -o "$installer"
# Inspect this script before executing it.
bash "$installer" --version v0.1.0 \
  --harness claude-code,codex,opencode --scope user --yes
# Or, instead of the preceding installation:
bash "$installer" --cli-only --yes --bin-dir "$HOME/.local/bin"
rm -f -- "$installer"
```

`--yes` requires explicit `--harness` and `--scope`, or `--cli-only`. To replace an
existing regular binary, additionally pass `--replace-binary`; symlink destinations
are refused. Binary publication is staged on the same filesystem. If skill setup
is cancelled or encounters a conflict, the verified binary remains installed and
the installer reports that setup did not complete. Retry `n4h setup` directly.

Checksums detect corruption; they are fetched from the same trusted GitHub release
as the binary and are **not an independent signature**. The bootstrap script on
`main` is mutable, even when `--version` pins the binary. For stricter reproducibility,
review a script from a trusted commit and use it with a pinned release. No remote
script should be run without trusting its source.

### Skills, not MCP

After either binary installation method:

```sh
n4h setup
n4h setup --harness codex,opencode --scope user --dry-run
n4h setup --harness codex,opencode --scope user --yes
n4h setup --harness claude-code --scope project --project /path/to/project --yes
n4h doctor
```

The portable skill is embedded in the binary from [`skills/n4h/SKILL.md`](skills/n4h/SKILL.md).
It teaches the agent to call n4h through the harness's existing terminal tool,
review plans, retain returned IDs and handle recovery and untrusted notes. It does
not register an MCP server, force tool usage, or grant terminal permissions.
Ask the agent explicitly to use the **n4h skill**. In Claude Code use `/n4h`,
in Codex mention `$n4h`, and in OpenCode V2 mention `@n4h`. Harness policies still apply.
OpenCode reloads skills automatically; use `/reload` if a forced reload is needed.
Other harnesses may require a new session to discover a newly installed skill.

| Harness | User scope | Project scope |
| --- | --- | --- |
| Claude Code | `~/.claude/skills/n4h/SKILL.md` | `.claude/skills/n4h/SKILL.md` |
| Codex | `~/.agents/skills/n4h/SKILL.md` | `.agents/skills/n4h/SKILL.md` |
| OpenCode V2 | `${XDG_CONFIG_HOME:-~/.config}/opencode/skills/n4h/SKILL.md` | `.opencode/skills/n4h/SKILL.md` |

Project scope means **the current directory**, unless `--project` supplies an
existing directory; it does not guess a repository root. User paths require an
absolute `HOME`; OpenCode honors an absolute `XDG_CONFIG_HOME` when set. Custom
harness skill sources/config roots are not auto-detected. Install manually if
your harness uses a different source. OpenCode also discovers Claude/Agents
compatibility directories; copies installed by this command are identical, but
native, project or explicitly configured sources may override the global skill.

Setup checks every destination before writing. Identical files are left alone;
different existing skills, nonregular files and symlink paths are refused, never
silently replaced. To upgrade a differing skill, inspect and back up/move your
existing file explicitly, then rerun setup. Each new skill is published without
clobbering an existing file. Multiple skill destinations are not one transaction:
an I/O failure may leave earlier skills installed; rerunning is safe. Paths must
be controlled by you; this is not protection against a hostile concurrent writer.

`setup` and `doctor` do not need `.n4h`, do not initialize/index projects, and never
edit `AGENTS.md`, `CLAUDE.md`, harness permission/config files or shell profiles.
Their stdout is bounded JSON; menus/previews go to the controlling terminal/stderr.
Token flags are supported; setup preflights its full success receipt before writes.

**PATH:** the installer adds its binary directory only to its own subprocesses.
It cannot change your parent shell or an already-running harness. Ensure the
binary directory is on the PATH used by both, then run `n4h doctor` there. It reports
the running executable, the first executable `n4h` on PATH and whether they match,
plus skill states (`missing`, `current`, `different`, `unsafe_or_unreadable`).
Missing skills are normal for unselected harnesses/CLI-only installation. A
successful doctor command means diagnostics ran, **not** that every integration
is installed, permitted or used by a model. Project checks cover the current
directory only, not ancestor/custom discovery sources.

To remove an integration, inspect and remove only the installed n4h skill file(s)
at the paths above. Remove the binary separately if desired. Do not delete whole
harness directories or repository `.n4h` state to uninstall the integration.

## Build

Requires a current stable Rust toolchain and a C compiler for native dependencies. Storage uses **redb**, an embedded database written in Rust; no database server or external `rg` binary is needed.

```sh
cargo build --locked
cargo test --locked --all-features
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Install the normal binary without test features:

```sh
cargo install --path . --locked
n4h setup
n4h doctor
```

Do **not** install with `--all-features`: `fault-injection` enables test-only process termination and simulated I/O failure points controlled by `N4H_TEST_FAILPOINT`. That environment variable has no effect in a normal build.

## Quick start

Run `init` at the root of the repository you want to edit:

```sh
n4h init
n4h find 'https://old.example/api' --literal --page-size 10
```

Every normal command emits one bounded JSON object on stdout, even in a terminal. `--json` is accepted explicitly for agent compatibility. Only `--help` and `--version` emit plain text. Never assume IDs from this example: capture the real values from each response.

```sh
n4h inspect r_ACTUAL_RESULT --match-id m_000001 --context 3

# Creates a plan and a diff, but does not edit source files.
n4h edit --on r_ACTUAL_RESULT --match-id m_000001 --replace 'https://new.example/api'

# Read any remaining diff pages before deciding to apply.
n4h inspect p_ACTUAL_PLAN
n4h apply p_ACTUAL_PLAN
n4h undo cp_ACTUAL_CHECKPOINT
```

To select several matches, repeat `--match-id`, including matches from different files. To select **all persisted matches**, including ones not shown on the first page, explicitly use `--scope all`. Targets are grouped by file and applied in stable path order. A single plan and checkpoint cover the whole changed set.

```sh
n4h edit --on r_ACTUAL_RESULT --match-id m_000001 --match-id m_000003 --replace 'new'
n4h edit --on r_ACTUAL_RESULT --scope all --replacement-file replacement.txt
```

`--replace`, `--replacement-file` and `--replacement-stdin` are mutually exclusive. Replacement content is captured **when the plan is created**. Changing a replacement file later cannot change an existing plan. An empty replacement deletes selected text.

```sh
printf 'first line\nsecond line' | n4h edit --on r_ACTUAL_RESULT --scope all --replacement-stdin
```

The replacement is always literal: `$1`, backslashes and escape sequences are not interpreted. Newline bytes are inserted as provided, not normalized to match the source file.

Matches whose bytes already equal the replacement are reported as `unchanged_matches` and do not produce writes. `selected_matches` counts the full selection; `changed_matches` counts the remaining matches in changed files. Files whose final bytes would be unchanged are excluded from the operation. If nothing changes, `edit` returns `no_changes`.

## Exact output token budgets

Use the global flags **together**, before or after subcommands:

```sh
n4h find 'old_api' --indexed --max-tokens 1200 --tokenizer o200k_base
n4h inspect r_ACTUAL_RESULT --cursor c_ACTUAL_CURSOR \
  --max-tokens 1200 --tokenizer cl100k_base
n4h resume --max-tokens 800 --tokenizer o200k_base
n4h mem list --max-tokens 1000 --tokenizer o200k_base
```

Supported encodings are `o200k_base` and `cl100k_base`, provided locally by `tiktoken-rs`. Tokenizer assets are bundled; no model API, runtime download or Python tokenizer is needed. The tokenizer is never inferred from the agent or model name. Both flags are required when either is present; the accepted token limit is **256–1,000,000**. The existing **64-KiB stdout limit** still applies independently, including its trailing newline. Without these flags the previous byte-only behavior remains, no `budget` field is added, and the BPE encoder is not initialized.

Every accepted bounded response includes:

```json
{
  "budget": {
    "max_tokens": 1200,
    "tokenizer": "o200k_base",
    "max_bytes": 65536,
    "output_tokens": 913,
    "count_scope": "serialized_json_plus_newline",
    "special_tokens": "ordinary_text"
  }
}
```

The number above is illustrative, not a token count for that standalone example. `output_tokens` in actual responses is computed from the **entire compact JSON serialization plus final newline**, including escaping, IDs, metadata and the count field itself. The count is stabilized to include its own numeric representation and checked again before emission. Special-token-looking repository text such as `<|endoftext|>` is counted as ordinary text, never silently collapsed to a reserved special token.

This is exact for the **selected encoding and n4h stdout**, not a promise about provider billing, chat/tool framing, hidden instructions, input tokens or a different model's tokenizer. Pretty-printing/reformatting the JSON changes the token count. There is no character-count heuristic enforcing this limit.

### Bounded views and pagination

- Searches (live, indexed and AST) reduce complete match units per page; total scope and match IDs do not change. Follow the regular cursor with either encoding or a different budget if desired.
- Memory list/search reduce complete notes per page. Full note text is not silently cut to make it fit. One note that cannot fit produces `output_budget_too_small`; use a larger budget or omit the flags.
- Plan file metadata has its independent `files_cursor_next`, as before. Both metadata and diff paging honor the selected token budget.
- Token-bounded diffs end on a **newline or EOF**, and expose `diff_chunk_boundary: "line_end"`. They can split a hunk into multiple pages, so concatenate chunks using `diff_start_byte`; a page is not necessarily a standalone patch. A single line that cannot fit fails explicitly rather than being cut or producing a cursor without progress. Byte-only diff pages retain their previous UTF-8 byte-chunk behavior. Following an old byte-only cursor with token flags can start the first new chunk mid-line, but its end still respects a line boundary.
- Context shrinks by whole lines, preserving the matched start line. `context_truncated` and `requested_context_radius` make reduction explicit; existing per-line snippet truncation flags remain visible.
- `resume` drops low-priority notes/recent history first. If only a compact summary fits, `details_truncated: true` is explicit and a pending recovery still returns `priority: "recovery_required"`, `recovery_required: true` and an operation ID to inspect. Notes never become generated next actions.
- Index status can similarly return a compact, explicitly truncated snapshot summary. A bounded build/update prepares its response before publishing; if detailed physical size is included, `physical_bytes_observed_before_commit: true` distinguishes the pre-commit observation from a later `index status` measurement.

If the minimum complete unit does not fit, n4h returns `output_budget_too_small` (exit 5). It never removes required IDs, changes an edit's selection, reports a smaller total as exhaustive, slices JSON/UTF-8, or invents token savings. A smaller output page is not a smaller edit scope. Very small budgets can be valid but still insufficient for a particular command's required metadata.

### Mutation safety and errors

`apply`, `undo` and `recover --rollback` serialize/count their exact intended success receipt **before journaling or modifying sources**, then return that prepared receipt after success. A budget refusal cannot apply changes and only afterwards report that the receipt was too large. Initialization and cache clearing also preflight before their effects; note create/update/delete preflight inside the revision-checked mutation. With token flags, new searches and plans check their initial output unit before saving references. An interrupted first cache build can still leave an unready disposable cache file, but a failed response preflight never publishes its staged generation.

Runtime errors keep their original exit/code and recovery operation ID. Long diagnostics can be compacted with `details_truncated` rather than overflowing the budget; inspect the operation before further source edits if an ID is present. A filesystem failure may still leave a partial operation, just as without token flags. Budget fitting is not a guarantee of stdout delivery: a broken pipe/lost response still requires inspecting state or using the documented idempotent retry.

If counting itself fails or corrupted local state contains an invalid oversized recovery identifier, a last-resort small fatal diagnostic can omit budget metadata and explicitly flag the invalid identifier. It never emits the enormous value as an unbounded fallback; valid generated recovery IDs are retained.

Malformed CLI invocations (missing paired flag, unsupported tokenizer, limit below 256), `--help` and `--version` are handled **before a token budget is accepted** and are exempt from that requested token limit. CLI errors remain byte-bounded JSON; help/version remain plain text. No source mutation occurs for rejected arguments.

Tokenization adds CPU/startup cost when requested, and paginating into many small responses repeats metadata; it does not automatically reduce total task tokens. There is no provider usage/cost claim without an external agent benchmark.

## Trigram index and incremental updates

Indexing is **explicit and optional**, for case-sensitive literal search only:

```sh
n4h index build
n4h index status
n4h find 'old_api' --indexed
n4h index update
n4h index update --verify
n4h index clear
```

`build` indexes the whole initialized repository root, even when invoked from a subdirectory. `update` reevaluates the whole-root path/ignore traversal, adding, changing and removing entries as needed. Query paths narrow the stored snapshot, with the normal current-directory default:

```sh
n4h find 'old_api' src tests --indexed
n4h find 'old_api' --indexed --page-size 2
```

Normal `find`, regex and AST search keep their live-scanning behavior. `--indexed --regex` is rejected; `ast find` has no indexed mode. There is no automatic fallback to a live scan when an index is missing, invalid or out of budget. Use `index build` or remove `--indexed` explicitly.

### Query semantics

The index stores complete admitted UTF-8 content plus file hashes and **distinct byte-trigram → file-ID postings**. A literal query intersects its postings, starting with the smallest posting lists, then verifies the exact full pattern with the same line-based matcher used by normal search. Postings can admit false-positive candidates, never replace exact matching. Patterns shorter than three **bytes** scan all cached files in the requested scope; UTF-8, BOM, CRLF, byte offsets and non-overlapping matches retain the normal search contract.

**An indexed result is a cached snapshot, not a live freshness guarantee.** Responses declare `scope: "indexed_snapshot"`, `index_generation`, `indexed_at`, the index identity and `verification_mode`. Files added or changed on disk after indexing are not reflected until update. A path deleted from disk can still be queried in that snapshot. Omissions and ignore policy describe the last build/update, not current filesystem coverage. Indexes do not expire automatically.

Indexed results use the normal persistent result IDs and pagination. `index.scope_files` counts admitted cached files in the query scope; `candidate_files` counts trigram candidates. `files_scanned` and `bytes_scanned` count cached candidate content verified by the exact matcher, not a traversal of current sources. Creating a query result still writes its handle to durable state.

Edits use ordinary `edit`/`apply`/`undo`: selected source hashes are verified against **real current files**. A changed/deleted target returns conflict, not automatic relocation or a stale overwrite. `--scope all` means all matches in the saved result, not all current repository matches. `inspect --match-id` also verifies the live file before showing context. Saved results and their cursors remain tied to their original snapshot even after updating or clearing the index.

### Incremental freshness and publication

- `index update` is the fast path: it still walks paths and ignore rules, but skips source-content reads for unchanged metadata. Fingerprints include size/mtime and, on Unix, device, inode, ctime, permission mode and link count. Missing timestamps force a reread.
- Files whose metadata changed are read/hashed. Unchanged content retains its postings while the fingerprint is refreshed. Files removed, newly ignored, oversized, binary or otherwise unsupported are removed from the admitted set.
- `index update --verify` rereads/hashes **all admitted source files**, even if fingerprints match. Use it when metadata may be unreliable. Fast updates do not promise detection of every conceivable metadata-preserving change.
- Each successful update publishes a new generation, even if content is unchanged. Status reports added/rewritten, deleted/excluded, reused, read-file and read-byte counters; `files_reused` includes verified-but-identical content. Full rebuilds rewrite entries rather than preserving postings.
- A single redb transaction publishes the manifest, content and postings together with `Durability::Immediate`. Failures, limits or termination before commit leave the previous whole generation visible. An interrupted first build has no ready snapshot and must be rebuilt.
- Traversal, ignore parsing or read errors **abort publication** instead of pretending the snapshot is exhaustive. Policy exclusions (hidden/ignored paths, binary/non-UTF-8 content, special files, oversized files) are reported separately. Individual files changing metadata during a read also abort publication.

As with ordinary search, reading a changing repository is not an atomic filesystem snapshot. Work without concurrent writers when building/verifying if you need a stable reference. Even a verified generation says what was read then, not what remains true now. Ignore rules are reevaluated on updates, not during cached queries.

### Separate disposable storage and limits

```text
.n4h/state.redb  # durable notes, results, plans, checkpoints and journal
.n4h/index.redb  # reconstructible content + postings cache
```

The cache uses the same repository process lock but a separate database and quota. It is created with Unix mode `0600` inside the private state directory. It is bound to the canonical root and repository identity; copying it into a different repository is rejected. Cache schema 1 is independent of the durable state schema. Unsupported/corrupt caches are not silently reset: explicitly clear/rebuild them. Source or cache symlinks/hardlinks are not followed.

Default cache budget: **512 MiB**, adjustable between 1 MiB and 2 GiB in bytes:

```sh
n4h index build --max-bytes 536870912
n4h index update --max-bytes 1073741824
```

The last committed budget is reused unless overridden. Limits remain 20,000 candidates, 2 MiB per source, 100 MiB candidate source bytes per build/update and **8 million posting pairs**. The index can occupy substantially more than source text; limits are not a claim that every 100-MiB repository fits the default cache budget. General source/search limits have not been raised automatically.

Capacity checks account for physical cache size, conservative copy-on-write estimates, staged allocated pages and commit headroom. A 32-MiB disk headroom check helps avoid consuming the space needed by durable operations. These are conservative checks, **not an OS disk reservation or power-loss certification**. Failed transactions/rebuilds can grow the file's high-water mark even though no generation was published; clearing/rebuilding or increasing the budget may be required. No automatic cache compaction is implemented.

Cache bytes do not count against the 200-MiB durable-state quota, but available filesystem space is shared. `index clear` deletes **only the designated regular cache file**, never notes, results, checkpoints, plans, journal or source files. It is available during pending source recovery and cannot dismiss that recovery. Clearing a cache is logical cleanup, not secure erasure. A subsequent fresh build has a new index identity even if its generation starts at 1.

**Privacy:** enabling indexing explicitly persists full contents of every admitted file, not just matches. Hidden/ignored exclusions are not a secret detector. The cache is not encrypted; do not use it where storing source copies is forbidden, and exclude all of `.n4h/` from Git yourself.

## Rust AST search and rewrites

`ast find` matches Rust syntax using **ast-grep + tree-sitter**, compiled into n4h with only the Rust grammar enabled. No external ast-grep binary, language server, Cargo invocation or network service is required.

```sh
# Quote $ variables so your shell does not expand them.
n4h ast find 'old_api($ARG)' src --page-size 10
n4h inspect r_ACTUAL_AST_RESULT --match-id m_000001 --context 3

# Different matches can expand $ARG to different original expressions.
n4h ast edit --on r_ACTUAL_AST_RESULT --scope all --replace 'new_api($ARG)'
n4h inspect p_ACTUAL_PLAN
n4h apply p_ACTUAL_PLAN
n4h undo cp_ACTUAL_CHECKPOINT
```

For example, `old_api(x)` and `old_api(a + b)` become `new_api(x)` and `new_api(a + b)` from one template. An unrelated comment or string containing `old_api(x)` is not a call-expression match. Patterns can span lines and match supported single Rust expressions, statements or items; this first version does not offer contextual selectors, YAML rules or arbitrary tree-sitter queries.

### Captures and templates

- `$NAME` captures one node. Names use uppercase letters, digits and underscores, starting with an uppercase letter.
- `$$$ARGS` captures a sequence, including an empty argument list. For example: `old_api($$$ARGS)` → `new_api($$$ARGS)`.
- Repeated names such as `check($A, $A)` constrain matching to structurally equal nodes. That is syntax equality, not proof of equal runtime values or resolved symbols.
- Templates must reference an actual capture in the same single/multiple form. Unknown captures or a mismatched form fail instead of inserting an empty value.
- `$$` emits a literal dollar sign in a template. `$1` is not a regex group, and `${...}` is not supported. Template substitution also occurs inside template string literals: escape dollars there when they should be literal.
- `--replacement-file` and `--replacement-stdin` work on `ast edit`; content is expanded and captured into the immutable plan at preparation time, never reread by `apply`.

```sh
n4h ast find 'old_api($$$ARGS)' src
n4h ast edit --on r_ACTUAL_AST_RESULT --match-id m_000001 \
  --replace 'new_api($$$ARGS)'
n4h ast edit --on r_ACTUAL_AST_RESULT --scope all --replacement-file rewrite.txt
```

Results have `mode: "ast_rust"`, with `ast.node_kind` and `ast.captures` on each match. Captures persist byte ranges and whether they are sequences, not copies of their complete text. The editor verifies the source hash, then expands from those stored ranges **without rerunning the search pattern**. `inspect` paginates the original result and can fetch verified context as usual. Pattern previews are bounded and declare `pattern_truncated` when shortened.

Capture bytes are copied exactly. For a sequence, the captured span runs from the first matched node through the last, retaining interior commas, comments and whitespace; empty sequences contribute no bytes. Whitespace/comments at capture boundaries or elsewhere inside the replaced match are not automatically retained. Only bytes **outside** replaced ranges are guaranteed untouched: review the diff. n4h does not run rustfmt or automatically reindent the rewrite.

### Syntax gates and scope

- Search only considers `.rs` files under the normal ignore/path/size policies. It parses complete files, not single lines. Invalid Rust according to the bundled grammar is reported as `rust_parse_error`, sets `coverage_complete=false`, and prevents planning from that result. Narrow the paths or fix the file and search again.
- After expanding **all selected replacements**, n4h parses each complete changed output. Syntax errors produce `ast_rewrite_invalid` and **no plan is saved**. Validation of one file never writes another.
- Nested/overlapping matches are returned for inspection, but cannot be selected together for editing. Use explicit, non-overlapping `--match-id` values; n4h does not silently choose inner or outer matches.
- Use `ast edit` for AST results. Normal `edit` rejects them rather than treating `$ARG` as literal by accident; `ast edit` rejects literal/regex results that lack captures. The normal `find`/`edit` replacement contract remains literal.
- Apply, whole-set preflight, checkpoints, idempotency, undo, per-file journal and rollback use the existing multi-file engine. AST matching does not weaken those protections.

**This is structural rewriting, not symbol-aware refactoring.** It does not resolve names, imports, scopes, traits or types; follow references when a declaration is renamed; expand Rust macros; evaluate cfg/features; or guarantee compilation or behavioral equivalence. A syntactically valid edit can still break the program. Macro contents are only exposed as represented by the parser, not as expanded Rust. Tree-sitter grammar acceptance can differ from rustc, especially for new/unsupported syntax. Run your project's `cargo check`/tests yourself after applying; n4h never runs repository commands automatically.

Limits: 16 KiB per pattern, 32 captured names and 64 KiB per unexpanded AST template. Expanded outputs still share the normal 50-file / 20-MiB plan limits. Search remains bounded to 2 MiB per file, 100 MiB per scan and 20,000 matches. These are resource bounds, not a latency guarantee for every structural pattern.

## Explicit agent memory

Operational state (results, plans, checkpoints and journal) is separate from **agent-authored notes**. Notes are repository-local data, not verified facts, trusted instructions or automatically executable commands. Nothing reads your conversation, environment or source files into notes automatically.

```sh
n4h mem put project.tests --text 'cargo test --locked --all-features'
n4h mem get project.tests
n4h mem list --page-size 10
n4h mem search 'cargo test' --page-size 10

# Use the actual revision returned by get, not a hardcoded value.
n4h mem put project.tests --text 'Updated test instructions' --if-revision ACTUAL_REVISION
n4h mem forget project.tests --if-revision ACTUAL_UPDATED_REVISION
```

`put` without `--if-revision` creates only: an existing key returns `note_exists`. Updates and deletions compare the requested revision with the current stored one and return `revision_conflict` rather than silently discarding work. Updating an absent key with a revision returns `note_not_found`.

Each note has `key`, `text`, `created_at`, `updated_at`, `revision` and `origin: "agent_note"`. Timestamps are Unix seconds. Revisions come from a monotonically increasing repository memory generation, **not a counter per key**: values can skip, and deleting/recreating a key does not reuse its old revision. `created_at` is retained on update, not on recreation. Even an explicit update with identical text advances the revision.

For multiline or quoting-sensitive text, choose exactly one input:

```sh
n4h mem put project.context --text-file notes.txt
printf 'First line\nSecond line' | n4h mem put project.context --text-stdin
```

Keys are 1–128 ASCII letters, digits, dots, underscores or hyphens. Text must be nonempty UTF-8, without NUL bytes, at most 8 KiB. Each repository permits 1,024 live notes and 4 MiB of combined key/text bytes, within the shared state quota. Memory has no automatic TTL.

`mem list` and `mem search` return full notes, sorted by key, with a requested page size of 1–100 (default 20), reduced as needed to fit 64 KiB JSON. Search is a **case-sensitive literal substring** over keys and text, not regex, embeddings or semantic search. A query must be 1–1,024 UTF-8 bytes without NUL. No search results or source code are searched by these commands.

Follow `cursor_next` using the **same command and query**:

```sh
n4h mem list --cursor c_ACTUAL_CURSOR
n4h mem search 'cargo test' --cursor c_ACTUAL_SEARCH_CURSOR
```

Memory cursors are bound to repository, query and memory generation. Any successful note create/update/delete invalidates prior cursors (`stale_cursor`): restart pagination without a cursor. Operational writes and failed note mutations do not invalidate memory cursors. Different repositories or queries reject each other's tokens. Memory pages are not independently persisted snapshots.

Memory operations use the repository lock and durable redb transactions. They remain available while source recovery is pending, but cannot alter or clear that journal. `forget` removes only the logical note and never removes plans, checkpoints, snapshots or journal records. It is **not secure erasure** and may not shrink the database file. Logical deletion can be attempted at the quota but still needs disk space to commit. Do not store secrets you cannot persist unencrypted.

## Resume between agent sessions

```sh
n4h resume
n4h resume --limit 5
```

`resume` is a read-only structured summary, with no model call and no automatic task continuation. It does not edit files, apply plans, repeat searches or execute note contents. Its top-level `priority`, `recovery_required` and `next_actions` prioritize pending source recovery over ordinary context and notes.

The response separates:

- `pending_operations`: operation/checkpoint IDs, statuses and file counts, with oldest pending work first.
- `operational_facts`: recent operations (including checkpoints), recent plans and their recorded application status, timestamps and expiration. These are `n4h_record` facts about history, **not a verification of current source bytes**. `file_state_verified` is explicitly `false`.
- `agent_notes`: recent note previews, marked `agent_note` with an explicit trust warning. Notes never become operational facts or generated next actions. Previews are capped at 240 Unicode characters; retrieve full content with `mem get`.

`--limit` bounds each section (1–50, default 10). Sections expose `total`, `shown` and `truncated`; output can drop lower-priority entries to respect the byte budget, preserving pending recovery first. Use `status`/`recover` for pending work, `inspect` with known IDs for plans and `mem list`/`search` for full paginated notes. `resume` is a bounded overview, not a paginated archive of all historical operations or plans. It scans stored metadata, not checkpoint blobs or the working tree.

Expired references can appear in a summary with `expired: true`; mentions in notes never extend their TTL. Plan expiry applies to unapplied use and inspection, not to durable checkpoints. A `committed` operation is historical: source files may have been changed externally afterwards. `resume` never infers that an agent task succeeded from a note or operation receipt.

## Search contract

- Literal search by default; `--regex` opts into Rust regex syntax (no look-around or backreferences).
- Search evaluates each line separately, excluding its CRLF/LF terminator. Literal newlines in the pattern and zero-length matches are rejected. Regex flags never enable cross-line matching.
- Directory walks respect ignore rules and omit hidden paths, `.git/` and `.n4h/`. The exclusions for `.git/` and `.n4h/` also apply to explicit paths.
- Symlinks are not followed. Files with multiple hardlinks, special permission bits, non-UTF-8 paths, NUL bytes or non-UTF-8 content are unsupported.
- Files over 2 MiB are omitted by policy and reported in `skipped`. Read/traversal errors mark `coverage_complete=false` and prevent planning from that result. Skipped-by-policy files are not included in the total match count.
- Results are ordered by relative path and byte offset, and saved before the first page is emitted. No suspended scan is hidden behind a cursor.
- `start_byte` and `end_byte` form a half-open byte range; `line` and `column_byte` are 1-based. UTF-8 BOM bytes count towards offsets/columns but are not searched as text.
- Search scope defaults to the current directory, while paths in responses are relative to the initialized root. Commands also work from its subdirectories.
- Use `--` before patterns starting with `-`: `n4h find --json -- '-old'`.

A result is a snapshot of the file versions read, **not an atomic snapshot of the repository**. Later new files and new matches are not added to it. `--scope all` means all matches in that saved result, not every current match in the working tree.

### Pagination and inspection

```sh
n4h find 'old' --page-size 2
n4h inspect r_ACTUAL_RESULT --cursor c_ACTUAL_CURSOR --page-size 2
n4h inspect r_ACTUAL_RESULT --match-id m_000001 --context 3
n4h inspect p_ACTUAL_PLAN --cursor c_ACTUAL_DIFF_CURSOR
n4h inspect p_ACTUAL_PLAN --cursor c_ACTUAL_FILES_CURSOR
```

Treat cursors as opaque. They are scoped to a result or plan, never resumable directory traversals. Pages do not rerun regex. Context inspection verifies that the source still matches the result hash. Diff pages read stored plan bytes, not current source files.

Multi-file plans report `files_total` and per-file paths, hashes, permission modes and selected-match counts. The diff concatenates each file's unified diff. Follow `cursor_next` for diff pages and, independently, `files_cursor_next` for remaining file metadata, passing either token to `inspect --cursor`. File-metadata continuation pages have `kind: "plan_files"`. An omitted summary or diff page never narrows the plan's write scope.

`truncated` describes a paged result. `scan_complete` describes completed traversal, while `coverage_complete` records whether that traversal encountered I/O errors. These are different concepts. At most 10 skipped details are emitted per result page, with a total count; detailed diagnostics beyond that sample are not pageable in this MVP.

## Safety model

1. `edit` verifies every selected file hash and captures immutable before/after bytes. No source changes are made; a conflict prevents saving the plan.
2. `apply` checks all snapshots and verifies the current hash and basic permissions of **every target before the first write**. A preflight conflict creates no operation and changes no sources.
3. One redb transaction durably commits the checkpoint, journal, snapshots for **all files** and lookup indexes before replacing any source. Every storage write uses `Durability::Immediate`.
4. Files are replaced sequentially using a synced temporary in each destination directory, another source hash check, rename and Unix directory sync. Per-file progress is durably recorded after replacement.
5. All target hashes are verified again before the operation is marked committed. An intermediate failure leaves a pending, potentially partially applied operation for explicit recovery; there is no automatic rollback that could hide or overwrite external work.
6. `undo` preflights **all** post-operation hashes and permissions before restoring any file. It restores the whole checkpoint as a new journalized, recoverable operation.

Reapplying a committed plan or repeating a completed undo returns the original recorded operation instead of editing again. This is a **historical receipt**, not a claim that no external edits have occurred since. A rolled-back operation is never silently retried; create a new plan.

Operation receipts report `files_total` and `files_modified_this_call`. To keep receipts bounded even with long paths, `paths` is a sample with `paths_truncated`; use the plan metadata or `recover` inspection to enumerate the entire operation. For backward compatibility, the single-file `path` field remains available; it is `null` for multiple files.

Existing uncommitted user changes are allowed and become part of the before snapshot. n4h never calls Git reset/checkout and never edits ignore files or agent instructions for you.

### What this does not guarantee

- No atomic multi-file filesystem transaction or semantic rename. Other processes can observe a partially applied or partially restored tree. redb commits internal state atomically, not the external files.
- The repository lock coordinates n4h processes, **not** editors or other writers. A race remains between the final hash check and rename. Work without concurrent writers, preferably in a dedicated worktree.
- Path checks and no-follow file opens reduce accidental symlink traversal; this is **not a sandbox for hostile working trees** or other processes running as your user.
- Basic Unix permission bits, BOM and untouched bytes are preserved. ACLs, xattrs, owner, timestamps and other platform-specific metadata are not preserved by contract. Read-only files are rejected.
- macOS/Windows, network filesystems and power-loss durability are not certified. Tests terminate real subprocesses at injected boundaries, not the kernel or storage device.
- An abrupt kill while writing a temporary can leave an orphan temporary alongside the source. Recovery protects/restores the source and journal, but this MVP does not automatically discover or delete such orphan temporaries.

## Interrupted operations

```sh
n4h status
n4h recover
n4h recover op_ACTUAL_OPERATION
n4h recover op_ACTUAL_OPERATION --cursor c_ACTUAL_OBSERVATIONS_CURSOR
n4h recover op_ACTUAL_OPERATION --rollback
```

Without `--rollback`, recovery is inspection-only. Pending operations block new plans, applies and undos; diagnostic searches, inspection, explicit note commands and `resume` remain available. Repository commands use the same repository lock and fail with `repository_busy` if another n4h process is active. Installation diagnostics and skill setup do not open repository state or take that lock.

Recovery inspects actual file hashes, not just the last journal update. `observations` reports per-file state and `recorded_progress`; follow `cursor_next` with `recover --cursor` if it is paged. Progress can lag a rename after a crash, and older records may report `unknown`; hashes remain authoritative. The `observation` shorthand is retained for single-file operations.

Rollback preflights **the entire operation**, accepting a mixture of before/after versions. If any file has a different version, missing file or unsupported permissions/type, no restorations begin. When preflight passes:

| Current source | Explicit rollback |
| --- | --- |
| Original hash and permissions | Leave that file unchanged. |
| Intended output hash and permissions | Restore that file's checkpoint bytes. |
| Different bytes/permissions, missing file, unsupported file | `recovery_conflict`; no overwrite or recreation. |

Only after all targets are back at their expected initial hashes is the operation marked `rolled_back`. If rollback itself is interrupted or a new conflict/I/O error appears during restoration, some files may already be restored: keep the journal and retry the same command after resolving the problem. Already-restored files are verified and left unchanged. Committed operations must use `undo`, not recovery rollback. There is no force-overwrite or complete-forward option. After a recovery conflict, preserve `.n4h/` and reconcile the external change manually; do not delete the pending checkpoint to bypass protection.

## Limits and state

| Resource | MVP limit |
| --- | --- |
| Per source file in search | 2 MiB |
| Files considered in a search | 20,000 |
| Bytes read per search | 100 MiB |
| Matches per result | 20,000 |
| Result page | 50 matches by default; 1–500 requested |
| JSON response, including newline | 64 KiB |
| Match preview / context line | 160 / 240 Unicode characters |
| Context radius | 0–20 lines |
| Files selected per plan | 50 |
| Plan before + after bytes, summed across all changed files | 20 MiB |
| Repository state quota | 200 MiB, with conservative journal/temp headroom |
| Result and unapplied plan TTL | 24 hours |
| Live memory notes | 1,024 |
| Per-note text / key | 8 KiB UTF-8 / 128 ASCII bytes |
| Combined live note key/text bytes | 4 MiB |
| Memory page / resume section | 20 notes / 10 items by default |
| Rust AST pattern / capture names | 16 KiB / 32 |
| Unexpanded AST template | 64 KiB |
| Separate index cache / posting pairs | 512 MiB default / 8 million |
| Optional output tokens | 256–1,000,000; explicit `o200k_base` or `cl100k_base` |

Large diffs are paginated without cutting UTF-8 or JSON. Search limits abort without saving an editable handle. A single response unit that cannot fit fails explicitly. Limits are fixed constants in this first release, not configurable flags.

State lives in `.n4h/state.redb`, with a separate lock file. The initialized canonical root is checked on every repository command: copying/moving `.n4h/` to another root is rejected. Linux creates the state directory with mode `0700` and the DB/lock with mode `0600`. `setup` and `doctor` bypass repository state entirely.

Metadata remains JSON; before/after snapshots are stored as binary values, not JSON byte arrays. The `meta`, `results`, `plans`, `snapshots`, `operations`, `checkpoint_index`, `source_index`, `pending_operations` and `notes` tables live in one redb database. A transaction updates an operation and its indexes together; repeated source/checkpoint keys are rejected without committing partial records. Status changes and the pending-operation index are also committed together. Note text and memory generation are updated atomically.

This protects the internal state, **not** the filesystem replacement: redb transactions do not remove the need for file hashes, the repository lock, checkpoints or `recover`. The storage backend is private to `Store`; no other production module accesses database tables directly. No performance improvement over SQLite is claimed without benchmarks.

### Existing redb state

Storage schema 6 reads schema-2 (single-file), schema-3 (multi-file), schema-4 (agent memory) and schema-5 (Rust AST) state without changing IDs, checkpoints or original snapshot keys. AST results have optional capture metadata; indexed results add optional snapshot provenance. Older records remain readable. Older databases with no memory table read as empty memory. Existing unapplied plans can still be inspected/applied; committed checkpoints can be undone; pending operations can be recovered. Read-only commands, including `mem list` and `resume`, do not upgrade the schema or create note tables. The first successful **durable-state** mutation upgrades the marker to 6 and initializes memory tables/generation if absent. Index maintenance affects only its separate cache; indexed queries also persist results and therefore upgrade durable state. Previous binaries that only understand schemas 2/3/4/5 will then reject the database: keep backups if you need to return to an older binary. The schema gate prevents older binaries from misinterpreting AST or indexed-snapshot results as ordinary live search results.

### Existing SQLite state

This version does **not** silently import, replace or delete old state. If `.n4h/state.sqlite3` or its WAL/SHM/journal sidecars exist, commands fail with `legacy_state_detected` (exit `3`), even if `state.redb` is also present. The check happens before acquiring/creating the n4h lock or opening either database.

Preserve the entire `.n4h/` directory and use the previous SQLite-backed binary to inspect/resolve pending operations before arranging an explicit migration. There is no SQLite importer in this release. Do not delete or rename old checkpoints just to get past detection; for a fresh demo, use a separate repository fixture instead.

**Privacy:** state contains explicit agent notes, patterns, match previews, full before/after plan bytes, and checkpoints; enabling indexing additionally caches complete admitted source contents. These can contain repository secrets. It is not encrypted, there is no secure-erasure guarantee, and there is no `--no-store` mode. Add `.n4h/` to your ignore policy yourself before committing anything. Do not use this MVP where persisting source snapshots or notes is prohibited.

TTL invalidates references; it does **not** erase rows or checkpoints. There is no automatic garbage collection in this version, and pending checkpoints are never expired. The quota can eventually prevent new operations; retention/export/cleanup commands are future work. Disk exhaustion during an operation is still an I/O failure requiring recovery, not a claim that preflight can reserve all filesystem space.

## JSON and exit codes

Successful results include `schema_version`, `ok: true` and `kind`. Errors include stable `error.code`, `message`, `paths`, `next_actions` and an operation ID when recovery is relevant. Messages may be shortened to honor output limits. Error objects use `retryable: false`: do not blindly repeat mutations; follow the suggested action or reconcile state first.

| Exit | Meaning |
| --- | --- |
| 0 | Success, including zero search matches and idempotent replay |
| 2 | Invalid CLI/selection, missing initialization |
| 3 | Conflict, expiry, missing reference, no changes, busy or recovery required |
| 4 | I/O, storage or stored-data integrity failure |
| 5 | Resource limit or unsupported capability/file |

Common codes: `scope_required`, `file_changed`, `plan_limit_exceeded`, `result_expired`, `plan_expired`, `invalid_cursor`, `search_limit_exceeded`, `storage_limit_exceeded`, `legacy_state_detected`, `recovery_required`, `recovery_conflict`. Conflict errors bound their path sample and report `paths_total` and `paths_truncated`.

Memory adds `note_exists`, `note_not_found`, `revision_conflict` and `stale_cursor` (exit 3), plus `memory_limit_exceeded` (exit 5). Invalid note keys/input options use exit 2; invalid stored note integrity uses exit 4.

AST adds `ast_result_required`, `ast_edit_required`, `unknown_capture` and malformed pattern/template errors (exit 2), `ast_rewrite_invalid` / `ast_parse_error` (exit 3), and `ast_limit_exceeded` (exit 5). `overlapping_matches` rejects an ambiguous edit selection (exit 3).

Index commands add `index_not_ready`, `invalid_index`, `index_scan_failed` (exit 3), `index_limit_exceeded`, `index_quota_exceeded` (exit 5). I/O or redb failures retain exit 4. A failed build/update does not silently downgrade to a partial snapshot.

Skill setup adds `confirmation_required`/`setup_cancelled` (exit 2), `skill_exists`/
`unsafe_setup_path` (exit 3), and `setup_write_failed` (exit 4). All destinations
are checked before writing, but an I/O failure during multiple installs may leave
earlier destinations installed; inspect the reported paths and retry safely.

Token and byte limits use `output_budget_too_small` (exit 5) for an indivisible response unit. `token_count_unstable` (exit 4) is a fail-closed counting error, never permission to emit an oversized success or write sources without a preflight receipt.

## Reproducible demo

Requires Python 3 only for the demo driver, not for n4h itself:

```sh
cargo build --locked
python3 scripts/demo.py --binary target/debug/n4h
python3 scripts/ast_demo.py --binary target/debug/n4h
# Optional: compile and run only the known, isolated AST demo fixture.
python3 scripts/ast_demo.py --binary target/debug/n4h --check-rust
python3 scripts/token_demo.py --binary target/debug/n4h
```

The script creates its own temporary multi-file fixture, captures actual IDs, verifies source bytes, demonstrates revision-guarded notes and `resume`, apply/undo across files and a rejected external change with no partial writes, and removes only its fixture. It never edits your project. Set `N4H_DEMO_TMPDIR` to select its temporary parent directory.

The AST demo separately shows two Rust files with different call arguments, unchanged comments/strings, rejected invalid syntax, apply/undo and whole-plan conflict protection. `--check-rust` requires `rustc` on PATH and explicitly compiles/runs the demo's own fixture before apply, after apply and after undo. This optional demo check is not automatic compilation by n4h.

The token demo exercises bounded search and diff pagination, a rejected too-small preview, unchanged edit scope, apply/undo and memory/resume. Its printed summaries describe the individual compact n4h responses, not the script's own output. Rust integration tests independently re-tokenize raw stdout with both encodings, including newline and metadata, and test Unicode, escaping, special-token-like text, compact diagnostics and mutation preflights.

### Reproducible index benchmark

```sh
cargo build --locked --release
python3 scripts/index_bench.py --binary target/release/n4h
# Example larger synthetic corpus within the current limits:
python3 scripts/index_bench.py --binary target/release/n4h --files 1024 --bytes-per-file 65536 --runs 5
```

The benchmark creates and removes its own temporary fixture, checks live/indexed match equality, alternates query order, records every CLI wall-time sample plus median/range, build and update times, physical cache size, postings, and source/cached bytes scanned. It checks that a no-op fast update reads zero source files, a one-file modification rereads one file, and `--verify` rereads all files. Timing includes process startup, JSON output and result persistence; OS caches are not cleared. It uses repetitive synthetic text and a selective query: results are exploratory, not proof of speedup on every repository. Peak memory and model tokens are explicitly **not measured**.

## Current scope vs. roadmap

Implemented: local state, literal/regex/Rust AST search, opt-in trigram-indexed literal snapshots, incremental/verified index updates with atomic generation publication, persisted captures and structural rewrite templates with output syntax validation, stable match references, byte- and optionally token-bounded result/diff/file-metadata pages, explicit selection, immutable multi-file plans, whole-set conflict and output-receipt preflights, apply/undo, idempotency, per-file journal progress, explicit partial-operation rollback recovery, repository-local agent notes with optimistic revisions, literal memory search and deterministic `resume`. Tests include exact self-inclusive token counts and mutation refusals, failed/interrupted index builds and updates, cache isolation and quotas, snapshot/live match parity, failures on the second source file, rollback failures, process termination between rename and journal, AST recovery and malformed rewrites, atomic note mutations, long-path/large-note pagination and backward compatibility with older redb records.

Not implemented: custom/non-tiktoken tokenizers, automatic model/tokenizer detection or provider billing estimates, indexed regex/AST search, filesystem watcher/daemon, cache compaction, symbol-aware rename/LSP/type-aware refactors, AST support for other languages, embeddings/semantic memory, automatic conversation capture, MCP, model benchmark, telemetry, operational-state retention commands or production cross-platform support. A Linux release installer/pipeline is provided; public installation still requires a published release. No token-saving percentage is claimed.

## Preparing a release (maintainers)

1. Review and publish the installer/setup changes. Keep the landing's release
   status accurate; do not remove its pending notice before the public flow works.
2. Confirm `Cargo.toml`/`Cargo.lock` package version (currently `0.1.0`) and choose
   its exact tag (`v0.1.0`). A different version/tag is rejected by packaging.
3. Run formatting, strict Clippy, all-feature tests, a **normal** release build,
   and `python3 scripts/test_install.py --binary target/release/n4h`.
4. On Linux x86-64, `python3 scripts/package_release.py --tag v0.1.0` rebuilds with
   `--locked --release --no-default-features --target x86_64-unknown-linux-gnu`.
   It never accepts a supplied test binary. `dist/` contains
   `n4h-x86_64-unknown-linux-gnu.tar.gz` (binary, README, skill) and `SHA256SUMS`.
   Local builds use the host's libc; the distributed build must use the workflow's
   Ubuntu 22.04 runner for the documented GNU libc baseline.
5. **Only when authorized**, create/push the version tag. The release workflow
   packages on Ubuntu 22.04, tests the actual archive with mocked network transport,
   uploads artifacts and creates a **draft** GitHub release. Alternatively dispatch
   it for an existing matching tag. It requires repository Actions permissions;
   only the publish job receives `contents: write`. Rerunning cannot overwrite an
   existing release; inspect/manage a failed draft explicitly.
6. Review the draft assets/checksums, publish it as a normal release (not merely a
   draft), then smoke-test pinned and `latest` public downloads in an isolated
   HOME/project. Only then update EN/ES landing/README publication notices.

The offline tests use a real binary/archive and isolated HOME/XDG/project paths,
not the developer's actual harness settings. They also exercise `curl | bash`
through a controlling PTY to verify that menus do not consume script stdin.
They do not prove that a live model will choose the skill or that GitHub assets
have been published. Tags, commits and publication are separate maintainer actions.

CI runs formatting, strict Clippy, process-level integration tests (including feature-gated crash tests), and a normal release build on Linux.
