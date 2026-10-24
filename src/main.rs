mod ast;
mod budget;
mod error;
mod fault;
mod files;
mod index;
mod memory;
mod model;
mod output;
mod plan;
mod resume;
mod search;
mod store;
mod transaction;

use budget::{Budget, Tokenizer};
use clap::{Parser, Subcommand, ValueEnum};
use error::{Error, Result};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::PathBuf;
use store::Store;

pub const SCHEMA_VERSION: &str = "0.3";

#[derive(Parser)]
#[command(
    name = "n4h",
    version,
    about = "Not for Human — verifiable text edits for coding agents"
)]
struct Cli {
    /// Accepted explicitly for agents; this MVP always emits JSON.
    #[arg(long, global = true)]
    json: bool,
    /// Bound the complete JSON response, including metadata and final newline.
    #[arg(long, global = true, requires = "tokenizer", value_parser = clap::value_parser!(u32).range(256..=1_000_000))]
    max_tokens: Option<u32>,
    /// Explicit output encoding; never inferred from an agent/model name.
    #[arg(long, global = true, requires = "max_tokens", value_enum)]
    tokenizer: Option<Tokenizer>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Initialize private local state; does not edit ignore files or agent instructions.
    Init,
    /// Complete a bounded line-based search and persist its match references.
    Find {
        pattern: String,
        paths: Vec<PathBuf>,
        #[arg(long, conflicts_with = "regex")]
        literal: bool,
        #[arg(long)]
        regex: bool,
        /// Query the committed index snapshot instead of walking source files.
        #[arg(long, conflicts_with = "regex")]
        indexed: bool,
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=500))]
        page_size: u16,
    },
    /// Page a result or immutable diff, or inspect one match's verified context.
    Inspect {
        id: String,
        #[arg(long, conflicts_with = "match_id")]
        cursor: Option<String>,
        #[arg(long)]
        match_id: Option<String>,
        #[arg(long, default_value_t = 3, value_parser = clap::value_parser!(u8).range(0..=20))]
        context: u8,
        #[arg(long, default_value_t = 50, value_parser = clap::value_parser!(u16).range(1..=500))]
        page_size: u16,
    },
    /// Create an immutable edit plan for up to 50 files; never write sources.
    Edit {
        #[arg(long)]
        on: String,
        #[arg(long, action = clap::ArgAction::Append, conflicts_with = "scope")]
        match_id: Vec<String>,
        #[arg(long, value_enum)]
        scope: Option<Scope>,
        #[arg(long, conflicts_with_all = ["replacement_file", "replacement_stdin"])]
        replace: Option<String>,
        #[arg(long, conflicts_with = "replacement_stdin")]
        replacement_file: Option<PathBuf>,
        #[arg(long)]
        replacement_stdin: bool,
    },
    /// Apply exactly the reviewed plan; reject changed files.
    Apply { plan_id: String },
    /// Restore one committed operation, only if its output remains unchanged.
    Undo { checkpoint_id: String },
    /// Inspect interrupted operations, or explicitly roll one back.
    Recover {
        operation_id: Option<String>,
        #[arg(long, requires = "operation_id")]
        rollback: bool,
        #[arg(long, requires = "operation_id", conflicts_with = "rollback")]
        cursor: Option<String>,
    },
    /// Report repository identity and pending recovery.
    Status,
    /// Explicit repository-local agent notes, separate from operational facts.
    Mem {
        #[command(subcommand)]
        command: memory::MemCommand,
    },
    /// Summarize recorded context; never continue tasks or modify source files.
    Resume {
        #[arg(long, default_value_t = 10, value_parser = clap::value_parser!(u16).range(1..=50))]
        limit: u16,
    },
    /// Structural Rust search and capture-based rewrites (not symbol resolution).
    Ast {
        #[command(subcommand)]
        command: ast::AstCommand,
    },
    /// Build, incrementally update, inspect or clear the disposable trigram index.
    Index {
        #[command(subcommand)]
        command: index::IndexCommand,
    },
}

#[derive(Clone, ValueEnum)]
enum Scope {
    All,
}

fn init_response(root: &std::path::Path, repository_id: &str) -> Value {
    let mut value = output::success("initialized");
    value["repository_id"] = json!(repository_id);
    value["root"] = json!(root);
    value["state_directory"] = json!(root.join(".n4h"));
    value["warning"] = json!(
        "State includes agent notes, search text, plans, full checkpoints and source content if indexing is enabled. Add .n4h/ to your ignore rules yourself. It is not encrypted."
    );
    value
}

fn run(cli: Cli, budget: &Budget) -> Result<Value> {
    let cwd = std::env::current_dir()?.canonicalize()?;
    if matches!(cli.command, Command::Init) {
        let store = Store::init_checked(&cwd, |root, id| {
            budget.finish(init_response(root, id)).map(|_| ())
        })?;
        return budget.finish(init_response(&store.root, &store.repository_id));
    }
    let store = Store::open(&Store::locate(&cwd)?)?;
    match cli.command {
        Command::Init => unreachable!(),
        Command::Find {
            pattern,
            paths,
            regex,
            indexed,
            page_size,
            ..
        } => {
            let result = if indexed {
                index::find(&store, &cwd, &pattern, &paths, budget)?
            } else {
                search::find(&store, &cwd, &pattern, regex, &paths, budget)?
            };
            output::result_page(&store, &result, None, page_size as usize, budget)
        }
        Command::Inspect {
            id,
            cursor,
            match_id,
            context,
            page_size,
        } => {
            if id.starts_with("r_") {
                let result = store.result(&id)?;
                if let Some(match_id) = match_id {
                    output::context(&store, &result, &match_id, context as usize, budget)
                } else {
                    output::result_page(
                        &store,
                        &result,
                        cursor.as_deref(),
                        page_size as usize,
                        budget,
                    )
                }
            } else if id.starts_with("p_") {
                if match_id.is_some() {
                    return Err(Error::invalid("--match-id applies only to search results."));
                }
                let data = store.plan(&id)?;
                Store::unexpired(data.plan.created_at, "plan_expired")?;
                output::plan_page(&data, cursor.as_deref(), budget)
            } else {
                Err(Error::invalid(
                    "inspect expects a result (r_) or plan (p_) ID.",
                ))
            }
        }
        Command::Edit {
            on,
            match_id,
            scope,
            replace,
            replacement_file,
            replacement_stdin,
        } => {
            let replacement = if let Some(text) = replace {
                text.into_bytes()
            } else if let Some(path) = replacement_file {
                bounded_read(std::fs::File::open(path)?)?
            } else if replacement_stdin {
                bounded_read(std::io::stdin().lock())?
            } else {
                return Err(Error::invalid(
                    "Specify --replace, --replacement-file or --replacement-stdin.",
                ));
            };
            let data = plan::prepare(
                &store,
                &on,
                &match_id,
                scope.is_some(),
                &replacement,
                budget,
            )?;
            output::plan_page(&data, None, budget)
        }
        Command::Apply { plan_id } => transaction::apply(&store, &plan_id, budget),
        Command::Undo { checkpoint_id } => transaction::undo(&store, &checkpoint_id, budget),
        Command::Recover {
            operation_id,
            rollback,
            cursor,
        } => transaction::recover(
            &store,
            operation_id.as_deref(),
            rollback,
            cursor.as_deref(),
            budget,
        ),
        Command::Status => transaction::status(&store, budget),
        Command::Mem { command } => memory::run(&store, command, budget),
        Command::Resume { limit } => resume::run(&store, limit as usize, budget),
        Command::Index { command } => index::run(&store, command, budget),
        Command::Ast { command } => match command {
            ast::AstCommand::Find {
                pattern,
                paths,
                page_size,
            } => {
                let result = ast::find(&store, &cwd, &pattern, &paths, budget)?;
                output::result_page(&store, &result, None, page_size as usize, budget)
            }
            ast::AstCommand::Edit {
                on,
                match_id,
                scope,
                replace,
                replacement_file,
                replacement_stdin,
            } => {
                let replacement = if let Some(text) = replace {
                    text.into_bytes()
                } else if let Some(path) = replacement_file {
                    bounded_read(std::fs::File::open(path)?)?
                } else if replacement_stdin {
                    bounded_read(std::io::stdin().lock())?
                } else {
                    return Err(Error::invalid("Specify a replacement template."));
                };
                let data = plan::prepare_ast(
                    &store,
                    &on,
                    &match_id,
                    scope.is_some(),
                    &replacement,
                    budget,
                )?;
                output::plan_page(&data, None, budget)
            }
        },
    }
}

fn bounded_read(reader: impl Read) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader
        .take(model::MAX_PLAN_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > model::MAX_PLAN_BYTES {
        return Err(Error::new(
            "plan_limit_exceeded",
            "Replacement exceeds 20 MiB.",
            5,
        ));
    }
    Ok(bytes)
}

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return;
            }
            emit(Error::invalid(error.to_string()).json(), 2);
        }
    };
    let budget = match Budget::new(cli.max_tokens, cli.tokenizer) {
        Ok(budget) => budget,
        Err(error) => emit(error.json(), error.exit),
    };
    match run(cli, &budget).and_then(|value| budget.finish(value)) {
        Ok(value) => emit(value, 0),
        Err(error) => emit(budget.error(&error), error.exit),
    }
}

fn emit(value: Value, exit: i32) -> ! {
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let result = serde_json::to_writer(&mut out, &value)
        .map_err(std::io::Error::other)
        .and_then(|_| out.write_all(b"\n"))
        .and_then(|_| out.flush());
    if result.is_err() {
        std::process::exit(4);
    }
    std::process::exit(exit);
}
