// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// A small Markdown subset for the assistant's answers (wave 8, strand E): paragraphs,
// headings, bullet and numbered lists, fenced code, and inline `code`, **bold** and *italic*.
// It yields plain data that the component renders as text nodes, so nothing a model writes
// ever becomes HTML (links stay text).

export interface Span {
  text: string;
  bold?: boolean;
  italic?: boolean;
  code?: boolean;
}

export type Block =
  | { kind: "p"; spans: Span[] }
  | { kind: "h"; spans: Span[] }
  | { kind: "ul" | "ol"; items: Span[][] }
  | { kind: "code"; text: string };

/** Inline markup of one line or paragraph. */
export function inline(text: string): Span[] {
  const out: Span[] = [];
  const re = /(`[^`]+`)|(\*\*[^*]+\*\*)|(__[^_]+__)|(\*[^*\s][^*]*\*)|(_[^_\s][^_]*_)/g;
  let last = 0;
  for (let m = re.exec(text); m; m = re.exec(text)) {
    if (m.index > last) out.push({ text: text.slice(last, m.index) });
    const t = m[0];
    if (m[1]) out.push({ text: t.slice(1, -1), code: true });
    else if (m[2] || m[3]) out.push({ text: t.slice(2, -2), bold: true });
    else out.push({ text: t.slice(1, -1), italic: true });
    last = m.index + t.length;
  }
  if (last < text.length) out.push({ text: text.slice(last) });
  return out;
}

export function parseMarkdown(source: string): Block[] {
  const blocks: Block[] = [];
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  let para: string[] = [];
  let list: { kind: "ul" | "ol"; items: string[] } | null = null;
  const flush = () => {
    if (para.length > 0) blocks.push({ kind: "p", spans: inline(para.join(" ")) });
    para = [];
    if (list) blocks.push({ kind: list.kind, items: list.items.map(inline) });
    list = null;
  };
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    const fence = /^\s*```/.exec(line);
    if (fence) {
      flush();
      const code: string[] = [];
      for (i++; i < lines.length && !/^\s*```/.test(lines[i]!); i++) code.push(lines[i]!);
      blocks.push({ kind: "code", text: code.join("\n") });
      continue;
    }
    const heading = /^\s{0,3}#{1,6}\s+(.*)$/.exec(line);
    const bullet = /^\s*[-*+]\s+(.*)$/.exec(line);
    const numbered = /^\s*\d+[.)]\s+(.*)$/.exec(line);
    if (line.trim() === "") {
      flush();
    } else if (heading) {
      flush();
      blocks.push({ kind: "h", spans: inline(heading[1]!) });
    } else if (bullet || numbered) {
      const kind = bullet ? "ul" : "ol";
      if (para.length > 0 || (list && list.kind !== kind)) flush();
      list ??= { kind, items: [] };
      list.items.push((bullet ?? numbered)![1]!);
    } else if (list && /^\s{2,}\S/.test(line)) {
      list.items[list.items.length - 1] += ` ${line.trim()}`;
    } else {
      if (list) flush();
      para.push(line.trim());
    }
  }
  flush();
  return blocks;
}
