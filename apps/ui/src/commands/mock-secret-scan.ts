// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// The mock backend's secret scanner (wave 5, strand E), for UI tests. A small stand-in for
// `rt_app::secret_scan` on the mock's request files (specs as JSON): `Bearer`/`Basic`
// credentials and well-known token formats in string values, and values of password, token
// and secret fields. `{{variables}}`, `***` and lines with `rt-scan:allow` are not findings.
// The real scanner (Rust, also in the browser through rt-wasm) is tested on its own.

import type { SecretFinding } from "./types";

const FORMATS: [RegExp, string][] = [
  [/\bgh[pousr]_[A-Za-z0-9]{36}\b/, "GitHub token"],
  [/\bxox[bpaesr]-[A-Za-z0-9-]{10,}/, "Slack token"],
  [/\bsk-[A-Za-z0-9_-]{20,}/, "API secret key"],
  [/\bAKIA[A-Z0-9]{16}\b/, "AWS access key id"],
  [/\beyJ[A-Za-z0-9_-]{8,}\.eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/, "JWT"],
];

const CREDENTIAL_KEY = /(?:password|passwd|secret|token|apikey|api_key)$/i;

const HINT = (name: string) =>
  `Make it a secret variable: in Environments add \`${name}\`, mark it as secret and write \`{{${name}}}\` here instead. Not a credential? Add \`# rt-scan:allow\` to the line.`;

const placeholder = (v: string) => v === "" || /^\{\{[^}]+\}\}$/.test(v.trim()) || v.includes("***");

function finding(path: string, line: number, kind: string, text: string, value: string, name: string): SecretFinding {
  return { path, line, kind, snippet: text.replace(value, "***").trim(), hint: HINT(name) };
}

export function mockScan(files: [string, string][]): SecretFinding[] {
  const out: SecretFinding[] = [];
  for (const [path, text] of files) {
    text.split("\n").forEach((line, i) => {
      if (line.includes("rt-scan:allow")) return;
      // `"key": "value"` or the mock's YAML `key: value` / `key: "value"`.
      const pair = /^\s*(?:- )?"?([A-Za-z_]+)"?:\s*(?:"((?:[^"\\]|\\.)*)"|([^"\s].*?))\s*,?\s*$/.exec(line);
      if (!pair) return;
      const key = pair[1];
      const raw = pair[2] ?? pair[3];
      const scheme = /^(Bearer|Basic)\s+(\S+)$/i.exec(raw);
      if (scheme && !placeholder(scheme[2])) {
        out.push(finding(path, i + 1, `${scheme[1].toLowerCase()} credential`, line, scheme[2], "token"));
        return;
      }
      const format = FORMATS.find(([re]) => re.test(raw));
      if (format) {
        out.push(finding(path, i + 1, format[1], line, raw.match(format[0])![0], "token"));
        return;
      }
      if (CREDENTIAL_KEY.test(key) && !placeholder(raw)) out.push(finding(path, i + 1, key, line, raw, key));
    });
  }
  return out;
}

/** As `rt_app::secret_scan::refusal`. */
export function mockRefusal(findings: SecretFinding[]): string {
  const files = new Set(findings.map((f) => f.path)).size;
  const f = findings[0];
  const n = findings.length;
  return `Commit refused: ${n} credential${n === 1 ? "" : "s"} in ${files} file${files === 1 ? "" : "s"} (${f.path}:${f.line} ${f.kind}). ${f.hint}`;
}
