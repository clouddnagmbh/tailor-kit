// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand F: the loaded part of the commit graph: pages of `GitApi.graph` (2000 commits
// each, more on demand while scrolling), their lanes (`layout.ts`, continued page by page) and
// the refs. Plain reactive state for GraphView.svelte; no DOM.

import type { GitApi } from "../api";
import type { GitGraphCommit, GitRef } from "../types";
import { LaneLayout, type LaneRow } from "./layout";

/** Commits per page (`rt_app::git::graph`: "pages of a few thousand"). */
export const PAGE = 2000;

export type GraphScope = "all" | "current";

export class GraphData {
  commits = $state.raw<GitGraphCommit[]>([]);
  rows = $state.raw<LaneRow[]>([]);
  refs = $state.raw<GitRef[]>([]);
  head = $state<string | null>(null);
  detached = $state(false);
  more = $state(false);
  loading = $state(false);
  /** The first page has arrived (or failed). */
  ready = $state(false);
  error = $state<string | null>(null);
  maxWidth = $state(1);
  readonly byFull = $derived(new Map(this.refs.map((r) => [r.full, r])));
  readonly index = $derived(new Map(this.commits.map((c, i) => [c.oid, i])));
  /** The current branch's short name, `null` while detached or without commits. */
  readonly branch = $derived(this.refs.find((r) => r.kind === "local" && r.current)?.name ?? null);

  private layout = new LaneLayout();
  /** Bumped on every reload, so a page that arrives late is dropped. */
  private generation = 0;
  private scope: GraphScope = "all";

  constructor(private readonly git: () => GitApi) {}

  /** Loads the first page again (after a Git action, a switch of the active collection or of
   *  the scope). */
  async reload(scope: GraphScope = this.scope): Promise<void> {
    this.scope = scope;
    const generation = ++this.generation;
    this.loading = true;
    this.error = null;
    try {
      const page = await this.git().graph({ skip: 0, limit: PAGE, ...(scope === "current" ? { refs: ["HEAD"] } : {}) });
      if (generation !== this.generation) return;
      this.layout = new LaneLayout();
      this.rows = this.layout.add(page.commits);
      this.maxWidth = Math.max(1, this.layout.maxWidth);
      this.commits = page.commits;
      this.refs = page.refs;
      this.head = page.head ?? null;
      this.detached = page.detached;
      this.more = page.more;
    } catch (e) {
      if (generation !== this.generation) return;
      this.error = e instanceof Error ? e.message : String(e);
      this.commits = [];
      this.rows = [];
      this.refs = [];
      this.more = false;
    } finally {
      if (generation === this.generation) {
        this.loading = false;
        this.ready = true;
      }
    }
  }

  /** Loads the next page, if there is one and none is loading. */
  async loadMore(): Promise<void> {
    if (!this.more || this.loading) return;
    const generation = this.generation;
    this.loading = true;
    try {
      const page = await this.git().graph({ skip: this.commits.length, limit: PAGE, ...(this.scope === "current" ? { refs: ["HEAD"] } : {}) });
      if (generation !== this.generation) return;
      this.rows = [...this.rows, ...this.layout.add(page.commits)];
      this.maxWidth = Math.max(1, this.layout.maxWidth);
      this.commits = [...this.commits, ...page.commits];
      this.refs = page.refs;
      this.more = page.more;
    } catch (e) {
      if (generation === this.generation) {
        this.error = e instanceof Error ? e.message : String(e);
        this.more = false;
      }
    } finally {
      if (generation === this.generation) this.loading = false;
    }
  }

  /** Loads pages until `oid` (or an id starting with it) is loaded; its index, or -1. */
  async find(oid: string, maxPages = 50): Promise<number> {
    for (let page = 0; ; page++) {
      const exact = this.index.get(oid);
      if (exact !== undefined) return exact;
      if (oid.length >= 4) {
        const i = this.commits.findIndex((c) => c.oid.startsWith(oid));
        if (i >= 0) return i;
      }
      if (!this.more || page >= maxPages) return -1;
      await this.loadMore();
    }
  }
}
