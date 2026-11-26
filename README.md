# Tailor Kit

Shared Rust crates and Svelte packages of [RequestTailor](https://github.com/clouddnagmbh/requesttailor)
and GitTailor. Apache-2.0. The products include it as the Git submodule `kit/` on a tag; a product
never changes `kit/` directly: change the kit, tag it, raise the submodule pointer.

| Path | What |
| --- | --- |
| `crates/tk-git-types` | Git errors, ref namespace, repository scope, seams for file mergers and commit scanners |
| `crates/tk-git` | The Git engine on gitoxide (no libgit2, no installed `git`) |
| `crates/tk-net` | Transport, streams, device-code prompt |
| `crates/tk-secrets` | Secret slots and the OS keychain |
| `crates/tk-scan` | Text secret scanner |
| `crates/tk-review` | Pull/merge requests on GitHub and GitLab |
| `crates/tk-ai` | AI providers, profiles, masking, prompts; context comes from the product |
| `packages/ui` | `@tailor-kit/ui`: primitives, tokens, icons |
| `packages/shell` | `@tailor-kit/shell`: command palette, dialogs, shortcuts, theme |
| `packages/git-ui` | `@tailor-kit/git-ui`: Git and review views behind a `GitHost`, Git mock as test backend |
| `packages/ai-ui` | `@tailor-kit/ai-ui`: AI settings, "what will be sent" preview, Markdown |

The kit knows no product: wording, file meaning and extra views come from the host interfaces
(`GitHost`, `AiHost`, `RefNamespace`, `FileMerger`, `CommitScanner`, `ScopeFiles`).

## Checks

```sh
cargo build --workspace --all-targets --locked
cargo test --workspace --locked --no-fail-fast
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build --workspace --exclude tk-git --target wasm32-unknown-unknown --locked
cargo deny --all-features check
npm ci --ignore-scripts && npm run check:licenses && npm run test:scripts
npm run check --workspaces && npm test --workspaces
node scripts/check-license-headers.mjs
node scripts/check-domain-words.mjs
node scripts/check-dco.mjs origin/main..HEAD
gitleaks git --no-banner --redact --config .gitleaks.toml .
ci/canary/run.sh
```

CI (`.github/workflows/ci.yml`) runs exactly these; `ci-ok` is the only required status.
See `CONTRIBUTING.md` for sign-off, headers and dependencies.
