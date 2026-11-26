// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { render } from "svelte/server";
import { describe, expect, it } from "vitest";

import Badge from "./Badge.svelte";
import Banner from "./Banner.svelte";
import Card from "./Card.svelte";
import EmptyState from "./EmptyState.svelte";
import Progress from "./Progress.svelte";
import SegmentedControl from "./SegmentedControl.svelte";

const html = (c: unknown, props: Record<string, unknown>) => render(c as never, { props: props as never }).body;

describe("attribute pass-through", () => {
  it("reaches the root of Badge, Card, Banner, EmptyState and SegmentedControl", () => {
    const attrs = { "data-testid": "x", "aria-describedby": "d", class: "extra" };
    const cases: [unknown, Record<string, unknown>, string][] = [
      [Badge, { value: "A", kind: "status" }, "rt-badge"],
      [Badge, { value: "GET", kind: "method" }, "rt-badge"],
      [Card, { title: "T" }, "rt-card"],
      [Card, { title: "T", onclick: () => {} }, "rt-card"],
      [Banner, { title: "T" }, "rt-banner"],
      [EmptyState, { title: "T" }, "rt-empty"],
      [SegmentedControl, { options: [{ value: "a", label: "A" }], value: "a", label: "Mode" }, "rt-segmented"],
    ];
    for (const [c, props, own] of cases) {
      const out = html(c, { ...props, ...attrs });
      expect(out).toContain('data-testid="x"');
      expect(out).toContain('aria-describedby="d"');
      expect(out).toMatch(new RegExp(`class="[^"]*${own}[^"]*extra`));
    }
  });

  it("keeps Card interactive with onclick", () => {
    const out = html(Card, { title: "T", onclick: () => {} });
    expect(out).toContain('role="button"');
    expect(out).toContain('tabindex="0"');
  });

  it("renders SegmentOption.testid as data-testid on that button", () => {
    const out = html(SegmentedControl, {
      options: [
        { value: "a", label: "A", testid: "seg-a" },
        { value: "b", label: "B" },
      ],
      value: "a",
      label: "Mode",
    });
    expect(out).toMatch(/<button[^>]*data-testid="seg-a"/);
    expect(out.match(/data-testid/g)).toHaveLength(1);
  });
});

describe("Badge status", () => {
  it("shows the letter by default and the word with long", () => {
    expect(html(Badge, { kind: "status", value: "M" })).toContain(">M<");
    const long = html(Badge, { kind: "status", value: "M", long: true });
    expect(long).toContain(">Modified<");
    expect(long).toContain("t-warn");
    expect(long).toContain("long");
  });
});

describe("Progress", () => {
  it("is a progressbar with value, min and max", () => {
    const out = html(Progress, { label: "Upload", value: 30, max: 60 });
    expect(out).toContain('role="progressbar"');
    expect(out).toContain('aria-label="Upload"');
    expect(out).toContain('aria-valuemin="0"');
    expect(out).toContain('aria-valuemax="60"');
    expect(out).toContain('aria-valuenow="30"');
    expect(out).toContain("width: 50%");
  });

  it("omits aria-valuenow when indeterminate", () => {
    const out = html(Progress, { label: "Loading" });
    expect(out).toContain('role="progressbar"');
    expect(out).not.toContain("aria-valuenow");
    expect(out).toContain("indeterminate");
  });
});
