// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// `ReviewApi` of the mock backend (wave 12, strand E): the model of the pull-request review lives
// in `@tailor-kit/git-ui/mock` (step K3). Here are RequestTailor's pull requests: #1 changes a
// request, adds a header to another and a new `Ping` request; a reviewer has already commented
// on a changed field.

import { mockReview as kitMockReview, MOCK_REVIEW_TOKEN, type MockReviewOptions } from "@tailor-kit/git-ui/mock";

import type { ReviewApi } from "./backend";
import type { MockWave12Host } from "./mock-wave12";
import type { FieldPath, RequestSpec, ReviewComment } from "./types";

export { MOCK_REVIEW_TOKEN };

const NOW = () => Math.floor(Date.now() / 1000);

export function mockReview(host: MockWave12Host): ReviewApi {
  const options: MockReviewOptions<RequestSpec> = {
    git: () => host.git(),
    saved: () => host.saved(),
    pullFiles: (saved) => {
      const files: Record<string, RequestSpec | null> = {};
      for (const [path, spec] of saved) files[path] = structuredClone(spec);
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
    },
    pulls: [
      { number: 1, title: "Add a deeper health check and a ping", author: "ada", draft: false, source: "feature/currency", target: "main", age: 3600 },
      { number: 2, title: "Draft: rename the users folder", author: "grace", draft: true, source: "chore/rename", target: "main", age: 3 * 86_400 },
    ],
    seed: async (session): Promise<ReviewComment[]> => {
      const diff = await host.backend.diffs.between(session.merge_base, session.head_ref);
      const change = diff.changes.find((c) => c.target.kind === "request" && c.fields.some((f) => f.line));
      const field = change?.fields.find((f) => f.line);
      if (!change || !field?.line) return [];
      return [
        {
          id: "2",
          author: "grace",
          body: "Is this URL right for staging?",
          created: NOW() - 5400,
          path: change.file,
          line: field.line,
          field: field.path as FieldPath,
          outdated: false,
        },
      ];
    },
    ci: {
      state: "failure",
      checks: [
        { name: "build", state: "success", url: "https://ci.example.test/build/1" },
        { name: "rt run", state: "failure", url: "https://ci.example.test/run/1" },
        { name: "lint", state: "pending", url: null },
      ],
    },
  };
  return kitMockReview(options);
}
