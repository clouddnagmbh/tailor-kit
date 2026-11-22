// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// `ReviewApi` of the mock backend (wave 12, strand E). Interface placeholder; the strand
// replaces it. `host` (./mock-wave12.ts) gives the mock's Git, requests, environments and
// sends; read it only inside the methods.

import type { ReviewApi } from "./backend";
import type { MockWave12Host } from "./mock-wave12";
import { stubReview } from "./wave12-stubs";

export function mockReview(host: MockWave12Host): ReviewApi {
  void host;
  return stubReview();
}
