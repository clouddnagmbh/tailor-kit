# Contributing

Thanks for helping. Before you open a pull request:

1. **Sign off every commit** (Developer Certificate of Origin, https://developercertificate.org).
   Use `git commit -s`, which adds `Signed-off-by: Your Name <you@example.org>`. The email
   must match the commit author. You keep your copyright; there is no CLA.
   Forgot it? `git rebase --signoff <base>` and force-push your branch.
2. **Start every new source file with the license header:**
   ```
   // SPDX-License-Identifier: Apache-2.0
   // Copyright 2026 The Tailor Kit Authors
   ```
   Files moved here from RequestTailor keep their line ("The RequestTailor Authors").
3. **No copyleft dependencies.** CI rejects any dependency whose license is not on the
   allowlist in `deny.toml` (no GPL, AGPL, LGPL, SSPL). Propose additions in the PR with a reason.
4. **No product words.** The kit serves several products; their domain stays in the products
   (`node scripts/check-domain-words.mjs`). Product wording comes from the host interfaces.
5. **Run the checks** listed in `README.md`; CI runs the same ones.

Contributions are accepted under the Apache License 2.0.
