# CLAUDE.md

Tailor Kit ist der gemeinsame Code von RequestTailor und GitTailor (Rust-Crates `tk-*`, Svelte-Pakete `@tailor-kit/*`), Apache 2.0, öffentlich. Die Produkte binden das Kit als Submodul `kit/` auf einem Tag ein.

## Regeln

- **Kein Produktwissen.** Kein Wort „Collection“ außerhalb von Kommentaren (`scripts/check-domain-words.mjs`). Wörter, Dateibedeutung und zusätzliche Ansichten kommen über die Gastgeber-Schnittstellen (`GitHost`, `AiHost`, `RefNamespace`, `FileMerger`, `CommitScanner`, `ScopeFiles`).
- **Änderungen zuerst hier.** Ein Produkt ändert `kit/` nie direkt: Änderung im Kit, CI grün, Tag, Submodul-Zeiger im Produkt anheben. Eine Änderung an einer Schnittstelle nennt im Commit, was die Produkte anpassen müssen.
- **Lizenzgrenze.** Keine Bezahlfunktion, keine Lizenzprüfung, keine Pro-Platzhalter. Keine Abhängigkeit mit GPL, AGPL, LGPL, SSPL oder anderer Copyleft-Lizenz; die Allowlist steht in `deny.toml` und gilt für Rust und npm. Neue Ausnahmen entscheidet der Projektverantwortliche, kein Agent. Code aus `requesttailor-pro` wird nicht gelesen und nicht übernommen.
- **Köpfe.** Neue Dateien: `// SPDX-License-Identifier: Apache-2.0` und `// Copyright 2026 The Tailor Kit Authors`. Aus RequestTailor verschobene Dateien behalten ihre Zeile.
- **DCO.** Jeder Commit mit `git commit -s`; die CI prüft das.
- **Verhalten.** Eine Kit-Änderung darf das Verhalten eines Produkts nur ändern, wenn das Produkt es im selben Zug bewusst übernimmt (neuer Tag, angepasste Tests im Produkt).

## Konventionen

- Code, Kommentare, Commit-Messages und öffentliche Doku auf Englisch; diese Datei auf Deutsch.
- `unsafe` ist workspace-weit verboten (`deny`).
- Crates erben `version`, `edition`, `license` und `[lints]` aus dem Workspace.
- Toolchain in `rust-toolchain.toml` gepinnt, gleicher Kanal wie in den Produkten.

## Befehle

Siehe `README.md`, Abschnitt „Checks“. Die CI führt genau diese Befehle aus.
