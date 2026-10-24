import type { Locale } from "./i18n";

export const repository = "https://github.com/tomiok/n4h";
export const documentation = `${repository}#quick-start`;
export const installCommand =
  "curl -fsSL https://raw.githubusercontent.com/tomiok/n4h/main/install.sh | bash";

export const sourceLinks = {
  readme: `${repository}#readme`,
  workflow: `${repository}#quick-start`,
  contract: `${repository}#json-and-exit-codes`,
  limits: `${repository}#limits-and-state`,
  safety: `${repository}#safety-model`,
  recovery: `${repository}#interrupted-operations`,
  scope: `${repository}#current-scope-vs-roadmap`,
  demos: `${repository}#reproducible-demo`,
  build: `${repository}#build`,
  benchmark: `${repository}#reproducible-index-benchmark`,
};

export const capabilityReferences = [
  `${repository}#search-contract`,
  `${repository}#trigram-index-and-incremental-updates`,
  `${repository}#rust-ast-search-and-rewrites`,
  `${repository}#exact-output-token-budgets`,
  `${repository}#explicit-agent-memory`,
  `${repository}#resume-between-agent-sessions`,
] as const;

export const recoveryCommands = {
  inspect: "n4h status\nn4h recover\nn4h recover op_OPERATION",
  rollback: "n4h recover op_OPERATION --rollback",
};

export const reproducibleCommands = {
  demos:
    "cargo build --locked\npython3 scripts/demo.py --binary target/debug/n4h\npython3 scripts/ast_demo.py --binary target/debug/n4h\npython3 scripts/token_demo.py --binary target/debug/n4h",
  checks:
    "cargo fmt --all -- --check\ncargo test --locked --all-features\ncargo clippy --locked --all-targets --all-features -- -D warnings\ncargo build --locked --release",
  index:
    "cargo build --locked --release\npython3 scripts/index_bench.py --binary target/release/n4h\npython3 scripts/index_bench.py --binary target/release/n4h \\\n  --files 1024 --bytes-per-file 65536 --runs 5",
};

export const quickStart = [
  {
    id: "install",
    code: installCommand,
  },
  {
    id: "source",
    code: `git clone ${repository}.git\ncd n4h\ncargo install --path . --locked`,
  },
  {
    id: "setup",
    code: "n4h setup\nn4h doctor\n# Noninteractive example:\nn4h setup --harness codex,opencode --scope user --dry-run\nn4h setup --harness codex,opencode --scope user --yes",
  },
  {
    id: "search",
    code: "cd /path/to/your/project\nn4h init\nn4h find 'https://old.example/api' --literal --page-size 10",
  },
  {
    id: "edit",
    code: "n4h inspect r_RESULT --match-id m_MATCH --context 3\nn4h edit --on r_RESULT --match-id m_MATCH \\\n  --replace 'https://new.example/api'\nn4h inspect p_PLAN\nn4h apply p_PLAN\nn4h undo cp_CHECKPOINT",
  },
  {
    id: "tokens",
    code: "n4h find 'old_api' --max-tokens 1200 --tokenizer o200k_base\nn4h inspect r_RESULT --cursor c_CURSOR \\\n  --max-tokens 1200 --tokenizer cl100k_base",
  },
  {
    id: "memory",
    code: "n4h mem put project.tests --text 'cargo test --locked'\nn4h mem get project.tests\nn4h resume --max-tokens 800 --tokenizer o200k_base",
  },
  {
    id: "index",
    code: "n4h index build\nn4h index status\nn4h find 'old_api' --indexed\nn4h index update\nn4h index update --verify\nn4h index clear",
  },
  {
    id: "ast",
    code: "n4h ast find 'old_api($ARG)' src --page-size 10\nn4h inspect r_AST_RESULT --match-id m_MATCH --context 3\nn4h ast edit --on r_AST_RESULT --match-id m_MATCH \\\n  --replace 'new_api($ARG)'\nn4h inspect p_PLAN\nn4h apply p_PLAN",
  },
] as const;

export const documentationGuides = [
  { id: "tokens", href: `${repository}#exact-output-token-budgets` },
  { id: "index", href: `${repository}#trigram-index-and-incremental-updates` },
  { id: "ast", href: `${repository}#rust-ast-search-and-rewrites` },
  { id: "memory", href: `${repository}#explicit-agent-memory` },
  { id: "safety", href: `${repository}#safety-model` },
] as const;

export type Benchmark = {
  name: Record<Locale, string>;
  n4h: string;
  baseline: string;
  context: Record<Locale, string>;
};

// Add measured results here. Publish methodology and environment alongside them.
// Do not use estimates or illustrative numbers in this array.
export const benchmarks: Benchmark[] = [];
export const benchmarkMethodologyUrl: string | null = null;

// Team-reported CLI task comparison supplied for the landing.
// Counts are PER PASS of 11 tasks, not summed across all five repetitions.
// "Tokens" are a bytes/4 estimate, not a tokenizer/provider measurement.
export const taskBenchmarkReport = {
  version: "0.1.0",
  commit: "d65fd47",
  repetitions: 5,
  tasksPerCondition: 11,
  conditions: [
    {
      id: "shell",
      marker: "A",
      estimatedTokens: 18181,
      toolCalls: 25,
      defectiveEdits: 1,
      completed: 10,
      safeRefusals: 0,
      incorrectAnswers: 0,
      timeMs: { median: 142, min: 137, max: 151 },
    },
    {
      id: "ripgrep",
      marker: "B",
      estimatedTokens: 18086,
      toolCalls: 25,
      defectiveEdits: 1,
      completed: 10,
      safeRefusals: 0,
      incorrectAnswers: 0,
      timeMs: { median: 219, min: 208, max: 231 },
    },
    {
      id: "n4h",
      marker: "C",
      estimatedTokens: 132884,
      toolCalls: 38,
      defectiveEdits: 0,
      completed: 7,
      safeRefusals: 2,
      incorrectAnswers: 2,
      timeMs: { median: 495, min: 492, max: 516 },
    },
  ],
} as const;

export const workflow = [
  {
    command: "find",
    lines: [
      {
        text: "$ n4h find 'https://old.example/api' --literal",
        type: "command",
      },
      {
        index: 0,
        type: "comment",
      },
      {
        text: "$ n4h inspect r_RESULT --match-id m_000001 --context 3",
        type: "command",
      },
      {
        index: 1,
        type: "comment",
      },
    ],
  },
  {
    command: "edit",
    lines: [
      {
        text: "$ n4h edit --on r_RESULT --match-id m_000001 \\",
        type: "command",
      },
      { text: "    --replace 'https://new.example/api'", type: "command" },
      {
        index: 0,
        type: "comment",
      },
      { text: "$ n4h inspect p_PLAN", type: "command" },
    ],
  },
  {
    command: "apply",
    lines: [
      { text: "$ n4h apply p_PLAN", type: "command" },
      { index: 0, type: "comment" },
      { index: 1, type: "comment" },
      {
        index: 2,
        type: "comment",
      },
    ],
  },
  {
    command: "undo",
    lines: [
      { text: "$ n4h undo cp_CHECKPOINT", type: "command" },
      { index: 0, type: "comment" },
      {
        index: 1,
        type: "comment",
      },
      {
        index: 2,
        type: "comment",
      },
    ],
  },
] as const;
