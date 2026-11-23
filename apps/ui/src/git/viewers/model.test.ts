// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

import { describe, expect, it } from "vitest";

import type { GitBlob } from "../../commands/types";
import { blobKind, dataUrl, diffRows, formatSize, parseLfsPointer, shortId, sizeChange } from "./model";

const blob = (over: Partial<GitBlob>): GitBlob => ({ path: "/f", size: 1, binary: false, truncated: false, lfs_pointer: false, ...over });

describe("blobKind", () => {
  it("tells text, images, pointers, binary and big files apart", () => {
    expect(blobKind(blob({ text: "a" }))).toBe("text");
    expect(blobKind(blob({ binary: true, media_type: "image/png", base64: "AA==" }))).toBe("image");
    expect(blobKind(blob({ media_type: "image/svg+xml", text: "<svg/>", base64: "AA==" }))).toBe("image");
    expect(blobKind(blob({ text: "version https://git-lfs.github.com/spec/v1\n", lfs_pointer: true }))).toBe("lfs");
    expect(blobKind(blob({ binary: true }))).toBe("binary");
    expect(blobKind(blob({ truncated: true }))).toBe("large");
  });
});

describe("sizes", () => {
  it("writes short sizes and changes", () => {
    expect(formatSize(12)).toBe("12 B");
    expect(formatSize(1536)).toBe("1.5 KB");
    expect(formatSize(5 * 1024 * 1024)).toBe("5.0 MB");
    expect(sizeChange(100, 100)).toBe("same size");
    expect(sizeChange(100, 1124)).toBe("+1.0 KB");
    expect(sizeChange(2048, 48)).toBe("-2.0 KB");
  });
  it("builds a data URL and shortens ids", () => {
    expect(dataUrl(blob({ media_type: "image/png", base64: "AAAA" }))).toBe("data:image/png;base64,AAAA");
    expect(shortId("sha256:0123456789abcdef", 6)).toBe("012345");
    expect(shortId(null)).toBe("");
  });
});

describe("parseLfsPointer", () => {
  it("reads oid and size, and nothing else", () => {
    const text = "version https://git-lfs.github.com/spec/v1\noid sha256:abc123\nsize 12345\n";
    expect(parseLfsPointer(text)).toEqual({ oid: "sha256:abc123", size: 12345 });
    expect(parseLfsPointer("just text")).toBeNull();
    expect(parseLfsPointer(null)).toBeNull();
  });
});

describe("diffRows", () => {
  it("gives context, hunks and line numbers", () => {
    const old = Array.from({ length: 20 }, (_, i) => `line ${i + 1}`).join("\n");
    const next = old.replace("line 10", "line ten");
    const rows = diffRows(old, next);
    expect(rows[0]).toEqual({ kind: "hunk", text: "@@ -7,7 +7,7 @@" });
    const removed = rows.find((r) => r.kind === "removed");
    const added = rows.find((r) => r.kind === "added");
    expect(removed).toMatchObject({ old: 10, new: null, text: "line 10" });
    expect(added).toMatchObject({ old: null, new: 10, text: "line ten" });
    expect(rows.filter((r) => r.kind === "context")).toHaveLength(6);
  });

  it("marks the words that changed in a paired line", () => {
    const rows = diffRows("GET /v1/users HTTP/1.1\n", "GET /v2/users HTTP/1.1\n");
    const removed = rows.find((r) => r.kind === "removed");
    const added = rows.find((r) => r.kind === "added");
    expect(removed && "words" in removed ? removed.words?.filter((s) => !s.same).map((s) => s.text) : null).toEqual(["v1"]);
    expect(added && "words" in added ? added.words?.filter((s) => !s.same).map((s) => s.text) : null).toEqual(["v2"]);
  });

  it("does not mark words when the whole line differs", () => {
    const rows = diffRows("abc\n", "xyz\n");
    expect(rows.every((r) => !("words" in r) || r.words === undefined)).toBe(true);
  });

  it("treats a new or deleted file as all added or removed", () => {
    expect(diffRows(null, "a\nb\n").filter((r) => r.kind === "added")).toHaveLength(2);
    expect(diffRows("a\nb\n", null).filter((r) => r.kind === "removed")).toHaveLength(2);
    expect(diffRows("same\n", "same\n")).toEqual([]);
  });
});
