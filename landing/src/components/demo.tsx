"use client";

import { useState } from "react";
import { workflow } from "@/lib/content";
import type { Dictionary } from "@/lib/i18n";

export function Demo({ copy }: { copy: Dictionary["demo"] }) {
  const [active, setActive] = useState(0);
  const step = workflow[active];
  const stepCopy = copy.steps[active];

  return (
    <div className="demo">
      <div className="demo-tabs" aria-label={copy.label}>
        {workflow.map((item, index) => (
          <button
            type="button"
            key={item.command}
            aria-pressed={active === index}
            aria-controls="command-example"
            className={active === index ? "demo-tab selected" : "demo-tab"}
            onClick={() => setActive(index)}
          >
            <span className="tab-number">0{index + 1}</span> {item.command}
          </button>
        ))}
      </div>
      <div
        id="command-example"
        className="demo-content"
        aria-live="polite"
        aria-atomic="true"
      >
        <div className="demo-heading">
          <div>
            <span className="eyebrow">0{active + 1} / 04</span>
            <h3>{stepCopy.title}</h3>
          </div>
          <p>{stepCopy.description}</p>
        </div>
        <div className="terminal">
          <div className="terminal-bar">
            <span className="terminal-dots" aria-hidden="true">
              <i />
              <i />
              <i />
            </span>
            <span>{copy.terminal}</span>
            <span>bash</span>
          </div>
          <pre>
            <code>
              {step.lines.map((line, index) => (
                <span
                  className={`code-line ${line.type}`}
                  key={`${active}-${index}`}
                >
                  {line.type === "command"
                    ? line.text
                    : stepCopy.comments[line.index]}
                  {"\n"}
                </span>
              ))}
            </code>
          </pre>
          <div className="terminal-note">
            <span aria-hidden="true">↳</span> {copy.note}
          </div>
        </div>
      </div>
    </div>
  );
}
