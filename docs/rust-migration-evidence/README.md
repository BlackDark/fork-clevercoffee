# Rust migration — evidence

What three independent efforts established about the C++ → Rust migration.

**TL;DR**

1. Only `rewrite/rust` has ever run on a board. Read its rows first.
2. Start with the verdict in each document. Details follow it.

| Doc | Purpose | Read |
|---|---|---|
| [ARCHITECTURE.md](ARCHITECTURE.md) | The recommended target architecture and its decision table | 4 min |
| [FINDINGS.md](FINDINGS.md) | Every peripheral and feature, with status and evidence | 5 min |
| [MIGRATION.md](MIGRATION.md) | C++ module → Rust module, and the remaining steps | 4 min |
| [STYLE.md](STYLE.md) | The writing rules these docs follow | 1 min |

Grades: ✅ reproduced on device · ⚠️ confirmed from source by two branches · ❓ single source or unexplained.

Branches read read-only: `rewrite/rust`, `refactor/space2`, `feat/rust-migration-design`. Nothing here is checked out.
