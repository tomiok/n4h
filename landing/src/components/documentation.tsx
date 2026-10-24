import { CodeSnippet } from "@/components/code-snippet";
import { documentation, documentationGuides, quickStart } from "@/lib/content";
import type { Dictionary } from "@/lib/i18n";

export function Documentation({ copy }: { copy: Dictionary["docs"] }) {
  return (
    <section
      className="section container documentation-section"
      id="documentation"
      aria-labelledby="documentation-title"
    >
      <div className="section-heading">
        <div>
          <p className="eyebrow">{copy.eyebrow}</p>
          <h2 id="documentation-title">
            {copy.title[0]}
            <br />
            {copy.title[1]}
          </h2>
        </div>
        <p>{copy.description}</p>
      </div>
      <p className="docs-requirements">
        <span aria-hidden="true">↳</span> {copy.requirements}
      </p>
      <div className="docs-layout">
        <div className="docs-steps">
          {quickStart.map((step, index) => {
            const text = copy.steps[step.id];
            return (
              <details className="docs-step" key={step.id} open={index === 0}>
                <summary>
                  <span className="docs-step-number" aria-hidden="true">
                    0{index + 1}
                  </span>
                  <span>{text.title}</span>
                  <span className="docs-expand" aria-hidden="true">
                    +
                  </span>
                </summary>
                <div className="docs-step-content">
                  <p>{text.description}</p>
                  <CodeSnippet
                    code={step.code}
                    title={text.title}
                    copy={copy.clipboard}
                  />
                  <p className="docs-step-note">{text.note}</p>
                </div>
              </details>
            );
          })}
        </div>
        <aside className="docs-sidebar" aria-label={copy.guidesTitle}>
          <h3>{copy.guidesTitle}</h3>
          <div className="docs-guides">
            {documentationGuides.map((guide) => (
              <a key={guide.id} href={guide.href}>
                <span>
                  <strong>{copy.guides[guide.id].title}</strong>
                  <span>{copy.guides[guide.id].description}</span>
                </span>
                <span aria-hidden="true">↗</span>
              </a>
            ))}
          </div>
          <a className="text-link" href={documentation}>
            {copy.reference} <span aria-hidden="true">↗</span>
          </a>
        </aside>
      </div>
      <div className="docs-contract">
        <div>
          <h3>{copy.contractTitle}</h3>
          <ul>
            {copy.limits.map((limit) => (
              <li key={limit}>{limit}</li>
            ))}
          </ul>
        </div>
        <div>
          <p>{copy.safety}</p>
          <p>{copy.state}</p>
        </div>
      </div>
    </section>
  );
}
