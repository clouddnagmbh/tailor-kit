// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 The RequestTailor Authors

// The UI primitives live in `packages/ui` (`@tailor-kit/ui`, shared with GitTailor). This
// re-export keeps `from "../ui"` working across RequestTailor.

export * from "@tailor-kit/ui";

import { registerIconAliases } from "@tailor-kit/ui";

// Extensions name the icon `collection`; the kit calls it `library`.
registerIconAliases({ collection: "library" });
