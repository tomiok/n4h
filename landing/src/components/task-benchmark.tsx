import { taskBenchmarkReport, repository } from "@/lib/content";
import type { Dictionary, Locale } from "@/lib/i18n";

const metrics = [
  "estimatedTokens",
  "toolCalls",
  "defectiveEdits",
  "completed",
  "safeRefusals",
  "incorrectAnswers",
] as const;

export function TaskBenchmark({
  copy,
  lang,
}: {
  copy: Dictionary["benchmarks"]["taskComparison"];
  lang: Locale;
}) {
  const report = taskBenchmarkReport;
  const number = new Intl.NumberFormat(lang === "es" ? "es-ES" : "en-US");

  return (
    <div className="task-benchmark" aria-labelledby="task-benchmark-title">
      <div className="benchmark-report-heading">
        <span className="badge">{copy.attribution}</span>
        <h3 id="task-benchmark-title">{copy.title}</h3>
        <p>{copy.description}</p>
        <div className="benchmark-metadata">
          <a href={`${repository}/releases/tag/v${report.version}`}>
            v{report.version} <span aria-hidden="true">↗</span>
          </a>
          <a href={`${repository}/commit/${report.commit}`}>
            {report.commit} <span aria-hidden="true">↗</span>
          </a>
          <span>
            {report.repetitions} {copy.runs}
          </span>
          <span>
            {report.tasksPerCondition} {copy.tasks}
          </span>
        </div>
      </div>
      <p className="benchmark-scroll-hint" id="benchmark-scroll-hint">
        {copy.scrollHint}
      </p>
      <div
        className="table-scroll task-benchmark-table"
        tabIndex={0}
        role="region"
        aria-label={copy.caption}
        aria-describedby="benchmark-scroll-hint"
      >
        <table>
          <caption>{copy.caption}</caption>
          <thead>
            <tr>
              <th scope="col">{copy.metric}</th>
              {report.conditions.map((condition) => (
                <th scope="col" key={condition.id}>
                  <span className="benchmark-condition">
                    {condition.marker}
                  </span>
                  {copy.conditions[condition.id]}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {metrics.map((metric) => (
              <tr key={metric} data-benchmark-metric={metric}>
                <th scope="row">{copy.metrics[metric]}</th>
                {report.conditions.map((condition) => (
                  <td
                    key={condition.id}
                    data-condition={condition.id}
                    data-value={condition[metric]}
                  >
                    {metric === "completed"
                      ? `${condition.completed}/${report.tasksPerCondition}`
                      : number.format(condition[metric])}
                  </td>
                ))}
              </tr>
            ))}
            <tr data-benchmark-metric="timeMs">
              <th scope="row">{copy.metrics.timeMs}</th>
              {report.conditions.map((condition) => (
                <td
                  key={condition.id}
                  data-condition={condition.id}
                  data-median={condition.timeMs.median}
                  data-min={condition.timeMs.min}
                  data-max={condition.timeMs.max}
                >
                  {condition.timeMs.median} ms{" "}
                  <span className="benchmark-range">
                    [{condition.timeMs.min}–{condition.timeMs.max}]
                  </span>
                </td>
              ))}
            </tr>
          </tbody>
        </table>
      </div>
      <p className="benchmark-token-note">{copy.tokenNote}</p>
      <h4 className="benchmark-subheading">{copy.findingsTitle}</h4>
      <div className="benchmark-findings">
        {copy.findings.map((finding) => (
          <article key={finding.title}>
            <h5>{finding.title}</h5>
            <p>{finding.description}</p>
          </article>
        ))}
      </div>
      <details className="benchmark-methodology">
        <summary>{copy.methodologyTitle}</summary>
        <ul>
          {copy.limitations.map((limitation) => (
            <li key={limitation}>{limitation}</li>
          ))}
        </ul>
      </details>
    </div>
  );
}
