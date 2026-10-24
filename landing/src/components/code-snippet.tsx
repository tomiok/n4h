"use client";

import { useState } from "react";
import type { Dictionary } from "@/lib/i18n";

export function CodeSnippet({
  code,
  title,
  copy,
}: {
  code: string;
  title: string;
  copy: Dictionary["docs"]["clipboard"];
}) {
  const [state, setState] = useState<"idle" | "copied" | "error">("idle");

  async function copyCommands() {
    try {
      await navigator.clipboard.writeText(code);
      setState("copied");
    } catch {
      setState("error");
    }
  }

  return (
    <div className="docs-snippet">
      <div className="docs-code">
        <div className="docs-code-bar">
          <span>bash</span>
          <button
            type="button"
            onClick={copyCommands}
            aria-label={`${copy.label}: ${title}`}
          >
            {state === "copied" ? copy.copied : copy.copy}
          </button>
        </div>
        <pre tabIndex={0} aria-label={title}>
          <code>{code}</code>
        </pre>
      </div>
      <p className="docs-copy-status" role="status">
        {state === "error" ? copy.error : state === "copied" ? copy.copied : ""}
      </p>
    </div>
  );
}
