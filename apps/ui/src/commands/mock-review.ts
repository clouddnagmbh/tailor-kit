// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// `ReviewApi` of the mock backend (wave 12, strand E): a small model of `rt_app::review` and its
// shells for UI tests and `?backend=mock`.
//
// - The mock's `origin` is `https://git.example.com/acme/demo-api.git`, a host the app does not
//   know: `repo()` is `null` until the user adds `git.example.com` under Git hosts, as with a
//   self-hosted host in the real app.
// - The only token the host accepts is `MOCK_REVIEW_TOKEN`; any other is an `auth` error, none
//   at all is `no-token`.
// - Two open pull requests. Opening #1 builds its head as a commit on `feature/currency` in the
//   mock's Git (the mock's own "remote push"): the demo collection with `Health` pointed at a
//   deeper check, a header added to `List users` and a new `Ping` request. Comments, verdicts
//   and the CI status are kept in memory.

import { BackendError, type ReviewApi } from "./backend";
import { MOCK_REMOTE_HOST } from "./mock-git";
import type { MockWave12Host } from "./mock-wave12";
import type {
  CiStatus,
  FieldPath,
  NewPullRequest,
  NewReviewComment,
  PullRequest,
  RequestSpec,
  ReviewComment,
  ReviewHost,
  ReviewRepo,
  ReviewSession,
  ReviewSubmission,
} from "./types";

/** The token the mock host accepts. */
export const MOCK_REVIEW_TOKEN = "mock-token";

const NOW = () => Math.floor(Date.now() / 1000);

export function mockReview(host: MockWave12Host): ReviewApi {
  let hosts: ReviewHost[] = [];
  const tokens = new Map<string, string>();
  let nextId = 100;
  const submitted: { number: number; submission: ReviewSubmission }[] = [];
  const sessions = new Map<number, ReviewSession>();
  const written = new Map<number, ReviewComment[]>();

  /** Pull requests opened with `create`, newest last. */
  const created: PullRequest[] = [];

  const pulls = (): PullRequest[] => [
    ...basePulls(),
    ...created,
  ];

  const basePulls = (): PullRequest[] => [
    {
      number: 1,
      title: "Add a deeper health check and a ping",
      author: "ada",
      state: "open",
      draft: false,
      source: "feature/currency",
      target: "main",
      head: "",
      base: "",
      url: `https://${MOCK_REMOTE_HOST}/acme/demo-api/pull/1`,
      updated: NOW() - 3600,
    },
    {
      number: 2,
      title: "Draft: rename the users folder",
      author: "grace",
      state: "open",
      draft: true,
      source: "chore/rename",
      target: "main",
      head: "",
      base: "",
      url: `https://${MOCK_REMOTE_HOST}/acme/demo-api/pull/2`,
      updated: NOW() - 3 * 86_400,
    },
  ];

  const find = (number: number): PullRequest => {
    const p = pulls().find((x) => x.number === number);
    if (!p) throw new BackendError("not-found", `pull request ${number}`);
    return p;
  };

  const detect = (): ReviewRepo | null => {
    const h = hosts.find((x) => x.host === MOCK_REMOTE_HOST);
    return h ? { kind: h.kind, host: h.host, api: h.api, project: "acme/demo-api" } : null;
  };

  /** Every call after the host is known needs the right token. */
  const authed = (): ReviewRepo => {
    const repo = detect();
    if (!repo) {
      throw new BackendError("not-found", "the origin of this collection is not on a Git host the app knows; add the host under Git hosts");
    }
    const token = tokens.get(repo.host);
    if (token === undefined) throw new BackendError("no-token", `no token for ${repo.host}`);
    if (token !== MOCK_REVIEW_TOKEN) throw new BackendError("auth", "401 Bad credentials");
    return repo;
  };

  const headFiles = (): Record<string, RequestSpec | null> => {
    const files: Record<string, RequestSpec | null> = {};
    for (const [path, spec] of host.saved()) files[path] = structuredClone(spec);
    const health = files["health.yml"];
    if (health) files["health.yml"] = { ...health, url: "{{baseUrl}}/health?deep=true" };
    const list = files["users/list-users.yml"];
    if (list) {
      files["users/list-users.yml"] = {
        ...list,
        headers: [...list.headers, { name: "X-Request-Source", value: "review", enabled: true }],
      };
    }
    files["ping.yml"] = {
      ...structuredClone(health ?? list!),
      name: "Ping",
      seq: null,
      url: "{{baseUrl}}/ping",
      headers: [],
      params: [],
    };
    return files;
  };

  async function open(number: number): Promise<ReviewSession> {
    authed();
    const pull = find(number);
    const cached = sessions.get(number);
    if (cached) return cached;
    const git = host.git();
    const base = await git.revParse("main");
    // Opening #2 shows the same collection with no changes: its head is the base.
    const head = number === 1 ? git.remoteCommit(headFiles(), pull.title, pull.source) : base;
    const session: ReviewSession = {
      pull: { ...pull, head, base },
      head_ref: head,
      merge_base: base,
    };
    sessions.set(number, session);
    return session;
  }

  /** The comments a reviewer would already have left: one on a changed field, one general. */
  async function seeded(number: number): Promise<ReviewComment[]> {
    if (number !== 1) return [];
    const session = await open(number);
    const diff = await host.backend.diffs.between(session.merge_base, session.head_ref);
    const change = diff.changes.find((c) => c.target.kind === "request" && c.fields.some((f) => f.line));
    const field = change?.fields.find((f) => f.line);
    const out: ReviewComment[] = [
      { id: "1", author: "grace", body: "Please look at the contract notes.", created: NOW() - 7200, path: null, line: null, outdated: false },
    ];
    if (change && field?.line) {
      out.push({
        id: "2",
        author: "grace",
        body: "Is this URL right for staging?",
        created: NOW() - 5400,
        path: change.file,
        line: field.line,
        field: field.path as FieldPath,
        outdated: false,
      });
    }
    return out;
  }

  const store = async (number: number): Promise<ReviewComment[]> => {
    let list = written.get(number);
    if (!list) {
      list = await seeded(number);
      written.set(number, list);
    }
    return list;
  };

  return {
    repo: async () => detect(),
    hosts: async () => structuredClone(hosts),
    setHosts: async (next) => {
      for (const h of next) {
        if (!h.host.trim() || !/^https?:\/\/./.test(h.api.trim())) {
          throw new BackendError("host", `the API address of ${h.host || "the host"} must start with https:// (or http://)`);
        }
      }
      hosts = next.map((h) => ({ host: h.host.trim().toLowerCase(), kind: h.kind, api: h.api.trim().replace(/\/+$/, "") }));
    },
    hasToken: async (h) => tokens.has(h),
    setToken: async (h, token) => {
      if (token.trim() === "") tokens.delete(h);
      else tokens.set(h, token.trim());
    },
    deleteToken: async (h) => void tokens.delete(h),
    startLogin: async () => {
      throw new BackendError("unsupported", "Signing in with a code is not available for this host; use a personal access token.");
    },
    finishLogin: async () => {
      throw new BackendError("unsupported", "Start the login first.");
    },
    list: async () => {
      authed();
      return pulls();
    },
    open,
    comments: async (number) => {
      authed();
      find(number);
      return structuredClone(await store(number));
    },
    comment: async (number, c: NewReviewComment) => {
      authed();
      find(number);
      const session = await open(number);
      const tree = await host.git().treeAt(session.head_ref);
      const text = tree.files[c.path];
      if (text === undefined || c.line < 1 || c.line > text.split("\n").length) {
        throw new BackendError("host", `422 Validation Failed (line ${c.line} of ${c.path} is not in the pull request)`);
      }
      const made: ReviewComment = {
        id: String(nextId++),
        author: "you",
        body: c.body,
        created: NOW(),
        path: c.path,
        line: c.line,
        outdated: false,
      };
      (await store(number)).push(made);
      return structuredClone(made);
    },
    submit: async (number, review) => {
      authed();
      find(number);
      if (review.verdict === "request-changes" && review.body.trim() === "") {
        throw new BackendError("host", "422 Validation Failed (Review body can't be blank)");
      }
      submitted.push({ number, submission: review });
    },
    ci: async (number): Promise<CiStatus> => {
      authed();
      find(number);
      return {
        state: "failure",
        checks: [
          { name: "build", state: "success", url: "https://ci.example.test/build/1" },
          { name: "rt run", state: "failure", url: "https://ci.example.test/run/1" },
          { name: "lint", state: "pending", url: null },
        ],
      };
    },
    create: async (pull: NewPullRequest) => {
      authed();
      if (pull.title.trim() === "") throw new BackendError("host", "422 Validation Failed (title can't be blank)");
      if (pull.head === pull.base) throw new BackendError("host", "422 Validation Failed (head and base are the same branch)");
      if (pulls().some((p) => p.source === pull.head && p.target === pull.base)) {
        throw new BackendError("host", `422 Validation Failed (A pull request already exists for ${pull.head})`);
      }
      const number = Math.max(0, ...pulls().map((p) => p.number)) + 1;
      const made: PullRequest = {
        number,
        title: pull.title,
        author: "you",
        state: "open",
        draft: pull.draft,
        source: pull.head,
        target: pull.base,
        head: "",
        base: "",
        url: `https://${MOCK_REMOTE_HOST}/acme/demo-api/pull/${number}`,
        updated: NOW(),
      };
      created.push(made);
      return structuredClone(made);
    },
  };
}
