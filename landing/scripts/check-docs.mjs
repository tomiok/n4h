import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

// Run against a local production server: npm run test:docs -- http://localhost:3094
const base = process.argv[2] || "http://localhost:3094";
const readme = await readFile(
  new URL("../../README.md", import.meta.url),
  "utf8",
);
const anchors = new Set(["readme"]);
for (const match of readme.matchAll(/^#{1,6} (.+)$/gm)) {
  anchors.add(
    match[1]
      .toLowerCase()
      .replace(/[^\p{L}\p{N}_ -]/gu, "")
      .replaceAll(" ", "-"),
  );
}

const root = await fetch(base, { redirect: "manual" });
assert.equal(root.status, 307);
assert.equal(root.headers.get("location"), "/en");

const sections = [
  "workflow",
  "capabilities",
  "documentation",
  "cli-contract",
  "safety",
  "benchmarks",
];
const commands = [
  "curl -fsSL https://raw.githubusercontent.com/tomiok/n4h/main/install.sh | bash",
  "n4h setup",
  "n4h doctor",
  "--harness codex,opencode --scope user --dry-run",
  "n4h index build",
  "n4h index update --verify",
  "n4h ast find",
  "n4h ast edit",
  "--max-tokens 1200 --tokenizer o200k_base",
  "n4h mem put",
  "n4h mem get",
  "n4h resume",
  "n4h recover op_OPERATION --rollback",
];
const drivers = ["demo.py", "ast_demo.py", "token_demo.py", "index_bench.py"];
for (const driver of drivers) {
  await access(new URL(`../../scripts/${driver}`, import.meta.url));
  assert.ok(
    readme.includes(`scripts/${driver}`),
    `Driver missing from README: ${driver}`,
  );
}

for (const lang of ["en", "es"]) {
  const response = await fetch(`${base}/${lang}`);
  assert.equal(response.status, 200);
  const html = await response.text();
  assert.match(html, new RegExp(`<html\\b[^>]*\\blang="${lang}"`));
  const hero = html.match(/<h1[^>]*>(.*?)<\/h1>/s)?.[1];
  assert.ok(hero?.includes("Not for human."));
  assert.ok(hero?.includes("Built for agents."));
  for (const section of sections)
    assert.ok(html.includes(`id="${section}"`), `Missing section: ${section}`);
  assert.equal((html.match(/class="feature"/g) || []).length, 6);
  assert.equal((html.match(/class="docs-step"/g) || []).length, 9);
  const heroSection = html.match(
    /<section[^>]*class="hero container"[^>]*>(.*?)<\/section>/s,
  )?.[1];
  assert.ok(
    heroSection?.includes(
      "curl -fsSL https://raw.githubusercontent.com/tomiok/n4h/main/install.sh | bash",
    ),
    "Installer command must be in the hero",
  );
  assert.ok(
    !html.includes(lang === "en" ? "release pending" : "release pendiente"),
  );
  for (const command of commands)
    assert.ok(html.includes(command), `Missing documented command: ${command}`);
  for (const driver of drivers) assert.ok(html.includes(`scripts/${driver}`));
  for (const unit of [
    "2 MiB",
    "20 MiB",
    "64 KiB",
    "100 MiB",
    "200 MiB",
    "512 MiB",
  ]) {
    assert.ok(readme.includes(unit));
    assert.ok(html.includes(unit), `Missing resource limit: ${unit}`);
  }
  for (const match of html.matchAll(
    /href="https:\/\/github\.com\/tomiok\/n4h#([^"\s]+)"/g,
  )) {
    assert.ok(
      anchors.has(match[1]),
      `Link does not match a README heading: ${match[1]}`,
    );
  }
  for (const removed of [
    "Sin magia.",
    "No magic.",
    "Evidence before",
    "Datos antes",
    'class="get-started"',
    'class="benchmark-visual"',
  ]) {
    assert.ok(
      !html.includes(removed),
      `Obsolete marketing content: ${removed}`,
    );
  }
  assert.ok(html.includes("recovery_conflict"));
  assert.ok(html.includes("output_budget_too_small"));
  assert.ok(html.includes("legacy_state_detected"));
  assert.ok(html.includes('hrefLang="en"') && html.includes('hrefLang="es"'));
  const expectedBenchmark = {
    estimatedTokens: [18181, 18086, 132884],
    toolCalls: [25, 25, 38],
    defectiveEdits: [1, 1, 0],
    completed: [10, 10, 7],
    safeRefusals: [0, 0, 2],
    incorrectAnswers: [0, 0, 2],
  };
  const conditions = ["shell", "ripgrep", "n4h"];
  for (const [metric, values] of Object.entries(expectedBenchmark)) {
    const row = html.match(
      new RegExp(
        `<tr[^>]*data-benchmark-metric="${metric}"[^>]*>(.*?)<\\/tr>`,
        "s",
      ),
    )?.[1];
    assert.ok(row, `Missing comparison row: ${metric}`);
    values.forEach((value, index) =>
      assert.match(
        row,
        new RegExp(
          `data-condition="${conditions[index]}"\\s+data-value="${value}"`,
        ),
      ),
    );
  }
  const timeRow = html.match(
    /<tr[^>]*data-benchmark-metric="timeMs"[^>]*>(.*?)<\/tr>/s,
  )?.[1];
  for (const [index, [median, min, max]] of [
    [142, 137, 151],
    [219, 208, 231],
    [495, 492, 516],
  ].entries()) {
    assert.match(
      timeRow,
      new RegExp(
        `data-condition="${conditions[index]}"\\s+data-median="${median}"\\s+data-min="${min}"\\s+data-max="${max}"`,
      ),
    );
  }
  conditions.forEach((condition, index) => {
    const outcomes =
      expectedBenchmark.completed[index] +
      expectedBenchmark.safeRefusals[index] +
      expectedBenchmark.incorrectAnswers[index] +
      expectedBenchmark.defectiveEdits[index];
    assert.equal(
      outcomes,
      11,
      `Outcome categories must total 11 tasks for ${condition}`,
    );
  });
  assert.ok(html.includes("bytes/4"));
  assert.ok(html.includes("d65fd47"));
  assert.ok(
    html.includes(
      lang === "en"
        ? "RESULTS REPORTED BY THE TEAM"
        : "RESULTADOS REPORTADOS POR EL EQUIPO",
    ),
  );
  assert.ok(html.includes(lang === "en" ? "7.3×" : "7,3 veces"));
  assert.ok(
    !html.includes(
      lang === "en"
        ? "No measured results published on this page."
        : "Esta página aún no publica resultados medidos.",
    ),
  );
  console.log(
    `${lang}: technical sections, commands, sources, benchmark values/caveats, and localization OK`,
  );
}

assert.equal((await fetch(`${base}/fr`)).status, 404);
console.log(
  "Documentation smoke checks passed. These checks do not execute n4h or verify benchmark performance.",
);
