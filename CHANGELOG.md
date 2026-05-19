# Changelog

All notable changes to this project will be documented in this file.

## [0.3.0] - 2026-05-19

### Features

- full monorepo support with path-based commit scoping [d5ab0a1](https://github.com/binary-birthday/release-ratchet/commit/d5ab0a1a71fc1d750c624953e7988c06bfa1395d)
- monorepo foundation — config, commit filtering, CLI flag, dispatch [79e4893](https://github.com/binary-birthday/release-ratchet/commit/79e48932e9ddb4a3cecf14ada459e497f679f654)

### Bug Fixes

- resolve clippy warnings for CI [a7e9f9e](https://github.com/binary-birthday/release-ratchet/commit/a7e9f9e496bb52412cb05cd9a1b436c3d437fb9f)
- review pass 4 — stable promotion, deny_unknown_fields, hooks, case validation [d74bb44](https://github.com/binary-birthday/release-ratchet/commit/d74bb449d25e182dd851ca412255395035ced4fb)
- monorepo review pass 3 — prefix stripping, root path, overlap check [fdd36da](https://github.com/binary-birthday/release-ratchet/commit/fdd36daabfa5ae09ec1834ac446c3a151f237f5f)
- monorepo review pass 2 — rollback, promotion, validation, hooks [164c4ef](https://github.com/binary-birthday/release-ratchet/commit/164c4ef00a0fe135fd14663ce5f6ad259e3ff784)
- monorepo review — prerelease support, dirty check, path validation [e702066](https://github.com/binary-birthday/release-ratchet/commit/e702066a42a46705775a1b424501e3e7b350cacf)

## [0.2.0] - 2026-05-19

### Features

- add remote-aware changelog links, completions, bump, cleanup, hooks, and check (5b604c9)
- add ecosystem auto-detection, notes command, and pre-release support (065d1b6)
- add backport command for cherry-picking fixes to maintenance branches (6fa4d49)

### Bug Fixes

- handle changelogs that start directly with a version heading (b2cc18e)
- node ecosystem uses brace-depth tracking to find top-level version (fabfe57)
- reject prerelease IDs with leading/trailing/consecutive dots (2c20617)
- validate prerelease ID, fix release regexes, error on missing --config (e3ada98)
- review findings — bugs, architecture, and test coverage (4fb352f)
- make command dispatch exhaustive, remove unreachable!() (bc6f734)
- backport review — working tree sync, tag perf, merge commit guard (3c3c75a)

## [0.1.1] - 2026-05-18

### Bug Fixes

- third review pass — ecosystem correctness and safety (a7ae456)

## [0.1.0] - 2026-05-18

### Features

- initial implementation of release-ratchet (5ac332e)

### Bug Fixes

- second review pass — robustness and correctness (1aa9ac9)
- address review findings across codebase (0e2452a)

[0.3.0]: https://github.com/binary-birthday/release-ratchet/compare/v0.2.0...v0.3.0
[0.2.0]: https://github.com/binary-birthday/release-ratchet/compare/v0.1.1...v0.2.0
[0.1.1]: https://github.com/binary-birthday/release-ratchet/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/binary-birthday/release-ratchet/releases/tag/v0.1.0
