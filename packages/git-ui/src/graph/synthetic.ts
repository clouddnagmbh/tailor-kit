// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// Wave 14, strand F: a deterministic, realistic history of any size (a main line, feature
// branches that fork and merge back, an occasional octopus merge, a second root), for the lane
// tests, the mock backend's large graph (`?backend=mock&graph=<n>`) and the graph budgets.

export interface SyntheticCommit {
  oid: string;
  parents: string[];
  summary: string;
  author: string;
  email: string;
  /** Unix seconds. */
  time: number;
}

export interface SyntheticRepo {
  /** Newest first, children before parents. */
  commits: SyntheticCommit[];
  /** Branch name → tip. `main` is the current branch. */
  branches: { name: string; oid: string }[];
  tags: { name: string; oid: string; message: string | null }[];
}

const PEOPLE = ["Ada Lovelace", "Grace Hopper", "Linus Torvalds", "Margaret Hamilton", "Ken Thompson", "Barbara Liskov", "Alan Turing"];
const VERBS = ["Add", "Fix", "Rename", "Document", "Tighten", "Split", "Speed up", "Clean up", "Test", "Use"];
const THINGS = [
  "the users endpoint",
  "pagination for orders",
  "the health check",
  "bearer auth for billing",
  "the invoice body",
  "retry headers",
  "the OpenAPI contract",
  "environment variables",
  "the login flow",
  "rate limit handling",
  "the webhook signature",
  "search filters",
];

/** A small seeded PRNG (mulberry32). */
function random(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function oidOf(n: number, seed: number): string {
  let h = (0x811c9dc5 ^ seed) >>> 0;
  let out = "";
  for (let round = 0; out.length < 40; round++) {
    for (const ch of `${seed}:${n}:${round}`) h = Math.imul(h ^ ch.charCodeAt(0), 0x01000193) >>> 0;
    out += h.toString(16).padStart(8, "0");
  }
  return out.slice(0, 40);
}

/** A history of `count` commits (at least 2); `firstSummary` is the message of the first. */
export function syntheticRepo(count: number, seed = 1, now = 1_790_000_000, firstSummary = "Start the project"): SyntheticRepo {
  const rnd = random(seed);
  const pick = <T>(list: readonly T[]) => list[Math.floor(rnd() * list.length)];
  const made: SyntheticCommit[] = [];
  const start = now - count * 1_800;
  const commit = (parents: string[], summary: string): string => {
    const oid = oidOf(made.length, seed);
    const author = pick(PEOPLE);
    made.push({
      oid,
      parents,
      summary,
      author,
      email: `${author.split(" ")[0].toLowerCase()}@example.com`,
      time: start + made.length * 1_800,
    });
    return oid;
  };
  let main = commit([], firstSummary);
  let docs: string | null = null;
  const docsAt = Math.max(2, Math.floor(count * 0.3));
  const features: { name: string; tip: string }[] = [];
  const merged: { name: string; tip: string }[] = [];
  const tags: SyntheticRepo["tags"] = [];
  let featureNo = 0;
  while (made.length < count) {
    const left = count - made.length;
    if (docs === null && made.length >= docsAt) {
      docs = commit([], "Start the documentation site");
      continue;
    }
    if (docs !== null && left > 1 && rnd() < 0.01) {
      docs = commit([docs], `Document ${pick(THINGS)}`);
      continue;
    }
    const r = rnd();
    if (r < 0.06 && features.length < 6) {
      featureNo++;
      features.push({ name: `feature/${featureNo}-${pick(THINGS).split(" ").pop()}`, tip: main });
      // The fork itself is no commit; the first commit on the branch follows.
      const f = features[features.length - 1];
      f.tip = commit([f.tip], `${pick(VERBS)} ${pick(THINGS)}`);
    } else if (r < 0.32 && features.length > 0) {
      const f = pick(features);
      f.tip = commit([f.tip], `${pick(VERBS)} ${pick(THINGS)}`);
    } else if (r < 0.42 && features.length > 0 && left > 1) {
      const i = Math.floor(rnd() * features.length);
      if (features.length >= 3 && rnd() < 0.05) {
        // An octopus merge of three branches.
        const three = features.splice(0, 3);
        main = commit([main, ...three.map((f) => f.tip)], `Merge ${three.map((f) => f.name).join(", ")}`);
        merged.push(...three);
      } else {
        const [f] = features.splice(i, 1);
        main = commit([main, f.tip], `Merge ${f.name} into main`);
        merged.push(f);
      }
    } else {
      main = commit([main], `${pick(VERBS)} ${pick(THINGS)}`);
      if (made.length % 500 === 0) tags.push({ name: `v${made.length / 500}.0`, oid: main, message: rnd() < 0.5 ? `Release ${made.length / 500}.0` : null });
    }
  }
  const branches = [{ name: "main", oid: main }, ...features.map((f) => ({ name: f.name, oid: f.tip }))];
  if (docs) branches.push({ name: "docs", oid: docs });
  if (merged.length > 0) {
    const last = merged[merged.length - 1];
    branches.push({ name: last.name, oid: last.tip });
  }
  return { commits: made.reverse(), branches, tags };
}

/** Just the commits of [`syntheticRepo`]. */
export function syntheticHistory(count: number, seed = 1): SyntheticCommit[] {
  return syntheticRepo(count, seed).commits;
}
