import { CodeSnippet } from "@/components/code-snippet";
import {
  recoveryCommands,
  reproducibleCommands,
  sourceLinks,
} from "@/lib/content";
import type { Dictionary } from "@/lib/i18n";

type ReferenceProps = {
  copy: Dictionary["technical"];
  clipboard: Dictionary["docs"]["clipboard"];
};

function SourceLink({
  href,
  children,
}: {
  href: string;
  children: React.ReactNode;
}) {
  return (
    <a className="text-link source-link" href={href}>
      {children} <span aria-hidden="true">↗</span>
    </a>
  );
}

export function TechnicalReference({ copy, clipboard }: ReferenceProps) {
  const contract = copy.contract;
  const recovery = copy.recovery;

  return (
    <>
      <section
        className="section container reference-section"
        id="cli-contract"
        aria-labelledby="contract-title"
      >
        <div className="section-heading">
          <div>
            <p className="eyebrow">{contract.eyebrow}</p>
            <h2 id="contract-title">
              {contract.title[0]}
              <br />
              {contract.title[1]}
            </h2>
          </div>
          <p>{contract.description}</p>
        </div>
        <div className="reference-grid">
          {contract.protocols.map((protocol) => (
            <article className="reference-card" key={protocol.title}>
              <h3>{protocol.title}</h3>
              <p>{protocol.description}</p>
            </article>
          ))}
        </div>
        <div className="reference-tables">
          <div className="reference-table">
            <h3>{contract.exitTitle}</h3>
            <div
              className="table-scroll"
              tabIndex={0}
              role="region"
              aria-label={contract.exitCaption}
            >
              <table>
                <caption>{contract.exitCaption}</caption>
                <thead>
                  <tr>
                    {contract.exitHeaders.map((header) => (
                      <th scope="col" key={header}>
                        {header}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {contract.exits.map((exit) => (
                    <tr key={exit.code}>
                      <th scope="row">
                        <code>{exit.code}</code>
                      </th>
                      <td>{exit.meaning}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <SourceLink href={sourceLinks.contract}>{copy.source}</SourceLink>
          </div>
          <div className="reference-table">
            <h3>{contract.limitsTitle}</h3>
            <div
              className="table-scroll"
              tabIndex={0}
              role="region"
              aria-label={contract.limitsCaption}
            >
              <table>
                <caption>{contract.limitsCaption}</caption>
                <thead>
                  <tr>
                    {contract.limitHeaders.map((header) => (
                      <th scope="col" key={header}>
                        {header}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {contract.limits.map((limit) => (
                    <tr key={limit.resource}>
                      <th scope="row">{limit.resource}</th>
                      <td>{limit.limit}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <SourceLink href={sourceLinks.limits}>{copy.source}</SourceLink>
          </div>
        </div>
      </section>

      <section
        className="safety-section"
        id="safety"
        aria-labelledby="safety-title"
      >
        <div className="container">
          <div className="section-heading">
            <div>
              <p className="eyebrow">{recovery.eyebrow}</p>
              <h2 id="safety-title">
                {recovery.title[0]}
                <br />
                {recovery.title[1]}
              </h2>
            </div>
            <p>{recovery.description}</p>
          </div>
          <div className="reference-grid safety-grid">
            {recovery.items.map((item) => (
              <article className="reference-card" key={item.title}>
                <h3>{item.title}</h3>
                <p>{item.description}</p>
              </article>
            ))}
          </div>
          <SourceLink href={sourceLinks.safety}>{copy.source}</SourceLink>
          <div className="recovery-commands">
            <article>
              <h3>{recovery.inspectTitle}</h3>
              <p>{recovery.inspectNote}</p>
              <CodeSnippet
                code={recoveryCommands.inspect}
                title={recovery.inspectTitle}
                copy={clipboard}
              />
            </article>
            <article>
              <h3>{recovery.rollbackTitle}</h3>
              <p>{recovery.rollbackNote}</p>
              <CodeSnippet
                code={recoveryCommands.rollback}
                title={recovery.rollbackTitle}
                copy={clipboard}
              />
            </article>
          </div>
          <SourceLink href={sourceLinks.recovery}>{copy.source}</SourceLink>
          <div className="scope-box">
            <h3>{recovery.scopeTitle}</h3>
            <p>{recovery.scopeDescription}</p>
            <ul>
              {recovery.exclusions.map((exclusion) => (
                <li key={exclusion}>{exclusion}</li>
              ))}
            </ul>
            <SourceLink href={sourceLinks.scope}>{copy.source}</SourceLink>
          </div>
        </div>
      </section>
    </>
  );
}

export function ReproducibleExamples({ copy, clipboard }: ReferenceProps) {
  const text = copy.reproducibility;
  const examples = [
    {
      title: text.demosTitle,
      description: text.demosDescription,
      code: reproducibleCommands.demos,
      href: sourceLinks.demos,
    },
    {
      title: text.checksTitle,
      description: text.checksDescription,
      code: reproducibleCommands.checks,
      href: sourceLinks.build,
    },
    {
      title: text.indexTitle,
      description: text.indexDescription,
      code: reproducibleCommands.index,
      href: sourceLinks.benchmark,
    },
  ];

  return (
    <div className="reproducible-examples">
      {examples.map((example) => (
        <article className="reproducible-example" key={example.title}>
          <div>
            <h3>{example.title}</h3>
            <p>{example.description}</p>
            <SourceLink href={example.href}>{copy.source}</SourceLink>
          </div>
          <CodeSnippet
            code={example.code}
            title={example.title}
            copy={clipboard}
          />
        </article>
      ))}
      <div className="measurement-scope">
        <h3>{text.limitsTitle}</h3>
        <ul>
          {text.limitations.map((limitation) => (
            <li key={limitation}>{limitation}</li>
          ))}
        </ul>
      </div>
    </div>
  );
}
