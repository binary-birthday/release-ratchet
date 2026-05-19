# Changelog

All notable changes to this project will be documented in this file.

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
