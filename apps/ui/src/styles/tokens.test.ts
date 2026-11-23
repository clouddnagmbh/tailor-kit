// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Guards for tokens.css: the two copies of the dark palette stay identical, both palettes
// define the same names, every v1 name still exists, and text pairs meet WCAG AA (4.5:1).

import { describe, expect, it } from "vitest";

import css from "./tokens.css?raw";

function block(selectorStart: string): Map<string, string> {
  const at = css.indexOf(selectorStart);
  expect(at, selectorStart).toBeGreaterThanOrEqual(0);
  const open = css.indexOf("{", at);
  const close = css.indexOf("}", open);
  const vars = new Map<string, string>();
  for (const m of css.slice(open + 1, close).matchAll(/(--rt-[a-z0-9-]+)\s*:\s*([^;]+);/g)) vars.set(m[1], m[2].trim());
  return vars;
}

const light = block(':root,\n[data-theme="light"]');
const darkOs = block(':root:not([data-theme="light"])');
const dark = block('\n[data-theme="dark"] {');
const scales = block(":root {");

function luminance(hex: string): number {
  const n = parseInt(hex.slice(1), 16);
  return [n >> 16, (n >> 8) & 255, n & 255]
    .map((c) => {
      const s = c / 255;
      return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
    })
    .reduce((sum, c, i) => sum + c * [0.2126, 0.7152, 0.0722][i], 0);
}

function contrast(a: string, b: string): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

describe("tokens v2", () => {
  it("keeps both copies of the dark palette identical", () => {
    expect([...darkOs]).toEqual([...dark]);
  });

  it("defines the same colour names in light and dark", () => {
    expect([...dark.keys()].sort()).toEqual([...light.keys()].sort());
  });

  it("still defines every v1 name", () => {
    const v1 = ["--rt-font-ui", "--rt-font-mono", "--rt-size", "--rt-size-small", "--rt-radius", "--rt-gap", "--rt-bar-height", "--rt-side-inset"];
    for (const name of v1) expect(scales.has(name), name).toBe(true);
    const v1Colours = ["--rt-bg", "--rt-bg-panel", "--rt-bg-hover", "--rt-bg-selected", "--rt-fg", "--rt-fg-muted", "--rt-border", "--rt-accent"];
    const more = ["--rt-accent-fg", "--rt-ok", "--rt-warn", "--rt-danger", "--rt-var-resolved", "--rt-var-secret", "--rt-var-unresolved"];
    const shell = ["--rt-focus", "--rt-bg-overlay", "--rt-method-get", "--rt-method-other", "--rt-lane-8", "--rt-diff-del-word"];
    for (const name of [...v1Colours, ...more, ...shell]) expect(light.has(name), name).toBe(true);
    expect(css).toContain("--rt-shadow: var(--rt-elev-3)");
  });

  it("has the scales of v2", () => {
    for (let i = 1; i <= 8; i++) expect(scales.has(`--rt-space-${i}`)).toBe(true);
    for (const n of ["sm", "md", "lg", "pill"]) expect(scales.has(`--rt-radius-${n}`)).toBe(true);
    for (const n of ["caption", "small", "body", "title", "heading"]) {
      expect(scales.has(`--rt-text-${n}`)).toBe(true);
      expect(scales.has(`--rt-leading-${n}`)).toBe(true);
      expect(scales.has(`--rt-font-${n}`)).toBe(true);
    }
    for (const n of ["sm", "md", "lg"]) expect(scales.has(`--rt-control-${n}`)).toBe(true);
    for (const n of ["--rt-ease", "--rt-dur-fast", "--rt-dur-normal", "--rt-focus-width"]) expect(scales.has(n)).toBe(true);
    for (const n of [1, 2, 3]) {
      expect(light.has(`--rt-elev-${n}`)).toBe(true);
      expect(light.has(`--rt-surface-${n}`)).toBe(true);
    }
    for (const tone of ["info", "ok", "warn", "danger"])
      for (const part of ["bg", "fg", "border"]) expect(light.has(`--rt-${tone}-${part}`)).toBe(true);
  });

  const pairs: [string, string][] = [
    ["--rt-fg", "--rt-bg"],
    ["--rt-fg", "--rt-bg-panel"],
    ["--rt-fg", "--rt-bg-selected"],
    ["--rt-fg-muted", "--rt-bg"],
    ["--rt-fg-muted", "--rt-bg-panel"],
    ["--rt-fg-muted", "--rt-surface-3"],
    ["--rt-fg-subtle", "--rt-bg"],
    ["--rt-accent", "--rt-bg"],
    ["--rt-accent-fg", "--rt-accent"],
    ["--rt-accent-solid-fg", "--rt-accent-solid"],
    ["--rt-accent-subtle-fg", "--rt-accent-subtle"],
    ["--rt-fg-on-solid", "--rt-danger-solid"],
    ["--rt-ok", "--rt-bg"],
    ["--rt-warn", "--rt-bg"],
    ["--rt-danger", "--rt-bg"],
    ["--rt-info-fg", "--rt-info-bg"],
    ["--rt-ok-fg", "--rt-ok-bg"],
    ["--rt-warn-fg", "--rt-warn-bg"],
    ["--rt-danger-fg", "--rt-danger-bg"],
    ["--rt-tooltip-fg", "--rt-tooltip-bg"],
    ["--rt-method-get", "--rt-bg"],
    ["--rt-method-post", "--rt-bg"],
    ["--rt-method-put", "--rt-bg"],
    ["--rt-method-patch", "--rt-bg"],
    ["--rt-method-delete", "--rt-bg"],
  ];

  for (const [name, palette] of [
    ["light", light],
    ["dark", dark],
  ] as const) {
    it(`meets WCAG AA for text in ${name}`, () => {
      for (const [fg, bg] of pairs) {
        const ratio = contrast(palette.get(fg)!, palette.get(bg)!);
        expect(ratio, `${fg} on ${bg} (${name}): ${ratio.toFixed(2)}`).toBeGreaterThanOrEqual(4.5);
      }
    });
  }
});
