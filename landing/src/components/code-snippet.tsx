"use client";

import { useState } from "react";
import type { Dictionary } from "@/lib/i18n";

export function CodeSnippet({
  code,
  title,
  copy,
  compact = false,
}: {
  code: string;
  title: string;
  copy: Dictionary["docs"]["clipboard"];
  compact?: boolean;
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

  const copyButton = (
    <button
      type="button"
      onClick={copyCommands}
      aria-label={`${copy.label}: ${title}`}
    >
      {state === "copied" ? copy.copied : copy.copy}
    </button>
  );

  return (
    <div className={compact ? "docs-snippet compact-snippet" : "docs-snippet"}>
      <div className="docs-code">
        {!compact && (
          <div className="docs-code-bar">
            <span>bash</span>
            {copyButton}
          </div>
        )}
        <pre tabIndex={0} aria-label={title}>
          <code>{code}</code>
        </pre>
        {compact && copyButton}
      </div>
      <p className="docs-copy-status" role="status">
        {state === "error" ? copy.error : state === "copied" ? copy.copied : ""}
      </p>
    </div>
  );
}
