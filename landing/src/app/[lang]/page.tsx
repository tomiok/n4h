import Link from "next/link";
import { notFound } from "next/navigation";
import { Demo } from "@/components/demo";
import { Documentation } from "@/components/documentation";
import {
  ReproducibleExamples,
  TechnicalReference,
} from "@/components/technical-reference";
import { dictionaries, isLocale, locales } from "@/lib/i18n";
import {
  benchmarkMethodologyUrl,
  benchmarks,
  capabilityReferences,
  repository,
  sourceLinks,
} from "@/lib/content";

const featureSymbols = ["find", "index", "ast", "≤ N", "mem", "resume"];
const sectionIds = [
  "workflow",
  "capabilities",
  "documentation",
  "cli-contract",
  "safety",
  "benchmarks",
];

function Arrow({ diagonal = false }: { diagonal?: boolean }) {
  return <span aria-hidden="true">{diagonal ? "↗" : "→"}</span>;
}

function GithubIcon() {
  return (
    <svg
      width="17"
      height="17"
      viewBox="0 0 24 24"
      fill="currentColor"
      aria-hidden="true"
    >
      <path d="M12 .8a11.5 11.5 0 0 0-3.64 22.41c.58.1.79-.25.79-.56v-2.22c-3.22.7-3.9-1.37-3.9-1.37-.53-1.34-1.29-1.7-1.29-1.7-1.05-.71.08-.7.08-.7 1.17.08 1.78 1.2 1.78 1.2 1.04 1.78 2.72 1.26 3.39.96.1-.75.4-1.26.74-1.55-2.57-.3-5.27-1.29-5.27-5.72 0-1.26.45-2.29 1.19-3.1-.12-.29-.52-1.47.11-3.06 0 0 .97-.31 3.16 1.18a11 11 0 0 1 5.75 0c2.2-1.49 3.16-1.18 3.16-1.18.63 1.59.23 2.77.11 3.06.74.81 1.19 1.84 1.19 3.1 0 4.44-2.7 5.42-5.28 5.71.42.36.79 1.07.79 2.16v3.23c0 .31.21.67.79.56A11.5 11.5 0 0 0 12 .8Z" />
    </svg>
  );
}

export function generateStaticParams() {
  return locales.map((lang) => ({ lang }));
}

export const dynamicParams = false;

export default async function Home({
  params,
}: {
  params: Promise<{ lang: string }>;
}) {
  const { lang } = await params;
  if (!isLocale(lang)) notFound();
  const copy = dictionaries[lang];

  return (
    <>
      <a className="skip-link" href="#main">
        {copy.navigation.skip}
      </a>
      <header className="site-header">
        <div className="container nav">
          <a
            className="wordmark"
            href={`/${lang}`}
            aria-label={copy.navigation.home}
          >
            n4h
            <span className="wordmark-cursor" aria-hidden="true">
              _
            </span>
          </a>
          <span className="nav-tagline">Not for Human</span>
          <nav aria-label={copy.navigation.label}>
            <a className="nav-workflow" href="#workflow">
              {copy.navigation.workflow}
            </a>
            <a className="nav-docs" href="#documentation">
              {copy.navigation.documentation}
            </a>
            <a href="#benchmarks">Benchmarks</a>
            <a className="nav-github" href={repository}>
              <GithubIcon />
              <span>GitHub</span>
              <Arrow diagonal />
            </a>
            <div
              className="language-switcher"
              role="group"
              aria-label={copy.navigation.languages}
            >
              {locales.map((locale) => (
                <Link
                  key={locale}
                  href={`/${locale}`}
                  scroll={false}
                  hrefLang={locale}
                  lang={locale}
                  className="language-link"
                  aria-label={
                    locale === "en" ? "Read in English" : "Leer en español"
                  }
                  aria-current={locale === lang ? "page" : undefined}
                >
                  {locale.toUpperCase()}
                </Link>
              ))}
            </div>
          </nav>
        </div>
      </header>

      <main id="main">
        <section className="hero container" aria-labelledby="hero-title">
          <div className="hero-topline">
            <span className="status-dot" /> {copy.hero.tagline}{" "}
            <span className="hero-version">MVP / RUST</span>
          </div>
          <h1 id="hero-title" lang="en">
            {copy.hero.title}
            <br />
            <span>{copy.hero.subtitle}</span>
          </h1>
          <p className="hero-description">{copy.hero.description}</p>
          <p className="hero-scope">{copy.hero.scope}</p>
          <div className="hero-actions">
            <a className="button button-primary" href="#documentation">
              {copy.hero.start} <Arrow />
            </a>
            <a className="button button-secondary" href={repository}>
              <GithubIcon /> {copy.hero.source}
            </a>
          </div>
          <div className="hero-bottom">
            <a className="hero-provenance" href={sourceLinks.readme}>
              {copy.hero.bottom} <Arrow diagonal />
            </a>
            <a href="#workflow">
              {copy.hero.explore} <span aria-hidden="true">↓</span>
            </a>
          </div>
          <div className="hero-cross cross-left" aria-hidden="true">
            +
          </div>
          <div className="hero-cross cross-right" aria-hidden="true">
            +
          </div>
        </section>

        <div className="principles-strip">
          <div className="container principles">
            <span>
              <span aria-hidden="true">⌕</span> {copy.principles[0]}
            </span>
            <span>
              <span aria-hidden="true">≡</span> {copy.principles[1]}
            </span>
            <span>
              <span aria-hidden="true">⌘</span> {copy.principles[2]}
            </span>
            <span>
              <span aria-hidden="true">↶</span> {copy.principles[3]}
            </span>
          </div>
        </div>

        <nav
          className="container section-index"
          aria-label={copy.technical.contentsLabel}
        >
          {sectionIds.map((id, index) => (
            <a href={`#${id}`} key={id}>
              <span aria-hidden="true">0{index + 1}</span>{" "}
              {copy.technical.contents[index]}
            </a>
          ))}
        </nav>

        <section
          className="section container"
          id="workflow"
          aria-labelledby="workflow-title"
        >
          <div className="section-heading">
            <div>
              <p className="eyebrow">{copy.workflow.eyebrow}</p>
              <h2 id="workflow-title">
                {copy.workflow.title[0]}
                <br />
                {copy.workflow.title[1]}
              </h2>
            </div>
            <p>{copy.workflow.description}</p>
          </div>
          <Demo key={lang} copy={copy.demo} />
          <a className="text-link source-link" href={sourceLinks.workflow}>
            {copy.technical.source} <Arrow diagonal />
          </a>
        </section>

        <section
          className="features-section"
          id="capabilities"
          aria-labelledby="features-title"
        >
          <div className="container">
            <div className="section-heading compact">
              <div>
                <p className="eyebrow">{copy.features.eyebrow}</p>
                <h2 id="features-title">
                  {copy.features.title[0]}
                  <br />
                  {copy.features.title[1]}
                </h2>
              </div>
            </div>
            <div className="feature-grid">
              {copy.features.items.map((feature, index) => (
                <article className="feature" key={feature.label}>
                  <span className="feature-symbol" aria-hidden="true">
                    {featureSymbols[index]}
                  </span>
                  <h3>{feature.title}</h3>
                  <p>{feature.description}</p>
                  <span className="feature-label">{feature.label}</span>
                  <a
                    className="feature-source"
                    href={capabilityReferences[index]}
                  >
                    {copy.features.source} <Arrow diagonal />
                  </a>
                </article>
              ))}
            </div>
            <p className="scope-note">
              <span aria-hidden="true">↳</span> {copy.features.scope}
            </p>
          </div>
        </section>

        <Documentation key={lang} copy={copy.docs} />
        <TechnicalReference
          copy={copy.technical}
          clipboard={copy.docs.clipboard}
        />

        <section
          className="section container"
          id="benchmarks"
          aria-labelledby="benchmarks-title"
        >
          <div className="section-heading">
            <div>
              <p className="eyebrow">{copy.benchmarks.eyebrow}</p>
              <h2 id="benchmarks-title">
                {copy.benchmarks.title[0]}
                <br />
                {copy.benchmarks.title[1]}
              </h2>
            </div>
            <p>{copy.benchmarks.description}</p>
          </div>
          <ReproducibleExamples
            copy={copy.technical}
            clipboard={copy.docs.clipboard}
          />
          {benchmarks.length > 0 ? (
            <div className="benchmark-results">
              <div className="table-scroll">
                <table>
                  <caption>{copy.benchmarks.caption}</caption>
                  <thead>
                    <tr>
                      {copy.benchmarks.columns.map((column) => (
                        <th scope="col" key={column}>
                          {column}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {benchmarks.map((result) => (
                      <tr key={result.name.en}>
                        <th scope="row">{result.name[lang]}</th>
                        <td>{result.n4h}</td>
                        <td>{result.baseline}</td>
                        <td>{result.context[lang]}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {benchmarkMethodologyUrl && (
                <a className="text-link" href={benchmarkMethodologyUrl}>
                  {copy.benchmarks.methodology} <Arrow diagonal />
                </a>
              )}
            </div>
          ) : (
            <div className="results-note">
              <span className="badge">{copy.benchmarks.pending}</span>
              <h3>{copy.benchmarks.pendingTitle}</h3>
              <p>{copy.benchmarks.pendingDescription}</p>
              <a className="text-link" href={sourceLinks.benchmark}>
                {copy.benchmarks.repository} <Arrow diagonal />
              </a>
            </div>
          )}
        </section>
      </main>

      <footer className="container footer">
        <div>
          <a
            className="wordmark"
            href={`/${lang}`}
            aria-label={copy.navigation.home}
          >
            n4h<span aria-hidden="true">_</span>
          </a>
          <span>{copy.footer.tagline}</span>
        </div>
        <div className="footer-links">
          <a href="#documentation">
            {copy.footer.documentation} <Arrow />
          </a>
          <a href={repository}>
            GitHub <Arrow diagonal />
          </a>
          <a href="#benchmarks">Benchmarks</a>
        </div>
        <span className="footer-bottom">{copy.footer.bottom}</span>
      </footer>
    </>
  );
}
