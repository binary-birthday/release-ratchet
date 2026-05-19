# release-ratchet

Automated semantic versioning and changelog generation from your commit history. Works with any git forge — GitHub, GitLab, Bitbucket, or plain git.

You write [conventional commits](https://www.conventionalcommits.org/). release-ratchet figures out the next version, writes your changelog, and tags the release.

## How it works

1. You commit using conventional commit messages (`feat:`, `fix:`, `docs:`, etc.)
2. `release-ratchet prepare` reads your commits, determines the version bump, updates your changelog and version files, and creates a release commit
3. You open a PR with that commit (or merge directly)
4. `release-ratchet release` tags the merged commit
5. Your CI publishes the release

No GitHub API tokens needed. No vendor lock-in. Just git.

## Install

**Pre-built binary (Linux x86_64):**

```sh
curl -fsSL https://raw.githubusercontent.com/binary-birthday/release-ratchet/main/install.sh | sh
```

Pin a specific version:

```sh
VERSION=v0.3.0 curl -fsSL https://raw.githubusercontent.com/binary-birthday/release-ratchet/main/install.sh | sh
```

**Build from source (macOS / Linux):**

Requires [Rust](https://rustup.rs/) 1.86+.

```sh
git clone https://github.com/binary-birthday/release-ratchet.git
cd release-ratchet
cargo build --release
cp target/release/release-ratchet /usr/local/bin/
```

On macOS you need Xcode Command Line Tools (`xcode-select --install`). If you hit OpenSSL errors:

```sh
brew install openssl pkg-config
export OPENSSL_DIR=$(brew --prefix openssl)
cargo build --release
```

**Shell completions:**

```sh
release-ratchet completions bash >> ~/.bashrc
release-ratchet completions zsh >> ~/.zshrc
release-ratchet completions fish > ~/.config/fish/completions/release-ratchet.fish
```

## Quick start

### First release

```sh
# Create a config (optional — works without one)
release-ratchet init

# See what's pending
release-ratchet status

# Prepare the release (bumps version, writes changelog, commits)
release-ratchet prepare --no-branch

# Tag it
release-ratchet release

# Push
git push && git push --tags
```

### Ongoing releases

After your first release, the workflow is the same. Make commits, then:

```sh
release-ratchet prepare --no-branch
release-ratchet release
git push && git push --tags
```

release-ratchet reads your commits since the last tag, determines whether it's a major, minor, or patch bump, and does the rest.

### With a PR workflow

If you use pull requests for releases:

```sh
# Creates a release branch with the changelog + version bump
release-ratchet prepare

# Push the branch and create a PR
git push -u origin release-ratchet--release
gh pr create --title "chore: release v1.2.0"

# After merging, tag the merge commit on main
git checkout main && git pull
release-ratchet release
git push --tags
```

## Commit conventions

release-ratchet follows the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
feat: add user authentication       → minor bump (0.1.0 → 0.2.0)
fix: handle null pointer            → patch bump (0.1.0 → 0.1.1)
feat!: redesign API                 → major bump (0.1.0 → 1.0.0)
docs: update readme                 → no bump
chore: update dependencies          → no bump
```

The `!` after the type (or a `BREAKING CHANGE:` footer) triggers a major version bump regardless of the commit type.

**All standard types:**

| Type | Bump | In changelog |
|---|---|---|
| `feat` | minor | Features |
| `fix` | patch | Bug Fixes |
| `perf` | patch | Performance |
| `revert` | patch | Reverts |
| `docs` | none | -- |
| `style` | none | -- |
| `refactor` | none | -- |
| `test` | none | -- |
| `build` | none | -- |
| `ci` | none | -- |
| `chore` | none | -- |

Scopes are supported: `feat(auth): add OAuth` or `fix(@myorg/utils): resolve import`.

## Version files

release-ratchet updates version numbers in your project files automatically:

| Ecosystem | File | How it's updated |
|---|---|---|
| Rust | `Cargo.toml` | `toml_edit` (preserves formatting and comments) |
| Node | `package.json` | Targeted string replacement (preserves formatting) |
| Python | `pyproject.toml` | `toml_edit` (preserves formatting) |
| Generic | Any file | Regex with capture group |

**Zero-config:** If you don't configure ecosystems, release-ratchet detects `Cargo.toml`, `package.json`, and `pyproject.toml` in your repo root automatically.

**Generic example** for a `version.txt` file:

```toml
[[ecosystems]]
type = "generic"
path = "version.txt"
pattern = 'VERSION=(\d+\.\d+\.\d+)'
```

## Config reference

Config file: `.release-ratchet.toml` (created by `release-ratchet init`).

```toml
# Tag prefix for version tags (e.g., "v" produces v1.2.3)
tag_prefix = "v"

# Your main branch name
main_branch = "main"

# Branch name created by `prepare` (without --no-branch)
release_branch = "release-ratchet--release"

# Path to the changelog file
changelog_path = "CHANGELOG.md"

# GPG sign release tags
sign_tags = false

# Delete release branch after `release` tags it
cleanup_branch = false

# Version files to update
[[ecosystems]]
type = "cargo"
path = "Cargo.toml"

# Override how commit types behave
[commit_type_overrides.refactor]
bump = "patch"
changelog = "Refactoring"

# Run commands after prepare or release
[hooks]
post_prepare = ["cargo check", "cargo test"]
post_release = ["cargo publish"]
```

All fields are optional. Sensible defaults are used for anything not specified.

## Pre-release versions

For alpha/beta/RC workflows:

```sh
# Start a pre-release cycle
release-ratchet prepare --no-branch --prerelease alpha
release-ratchet release
# → v1.0.0-alpha.1

# Iterate
release-ratchet prepare --no-branch --prerelease alpha
release-ratchet release
# → v1.0.0-alpha.2

# Switch to beta (resets the counter)
release-ratchet prepare --no-branch --prerelease beta
release-ratchet release
# → v1.0.0-beta.1

# Promote to stable (omit --prerelease)
release-ratchet prepare --no-branch
release-ratchet release
# → v1.0.0
```

## Backporting fixes

Cherry-pick a hotfix to an older version line:

```sh
# Backport commit abc1234 to the v1.x maintenance branch
release-ratchet backport abc1234 --onto v1.2.0

# Prepare and release on the maintenance branch
release-ratchet prepare --no-branch
release-ratchet release
# → v1.2.1
```

This creates a `maintain/v1.2.x` branch from the tag, cherry-picks the commit, and lets you release a patch on the old version line.

## Monorepo support

For repositories with multiple independently-versioned packages:

```toml
[[packages]]
name = "core"
path = "packages/core"
tag_prefix = "core-v"

[[packages.ecosystems]]
type = "cargo"
path = "packages/core/Cargo.toml"

[[packages]]
name = "cli"
path = "packages/cli"
tag_prefix = "cli-v"

[[packages.ecosystems]]
type = "cargo"
path = "packages/cli/Cargo.toml"

# Optional: shared code that affects multiple packages
[[shared_paths]]
path = "utils"
affects = ["core", "cli"]
```

Commits are attributed to packages based on which files they touch. A commit modifying `packages/core/src/lib.rs` bumps `core`. A commit touching files in both packages bumps both.

```sh
# Status for all packages
release-ratchet status

# Status for one package
release-ratchet -p core status

# Prepare all packages with changes
release-ratchet prepare --no-branch

# Release creates a tag per package (core-v1.0.0, cli-v0.5.0)
release-ratchet release
```

## CI integration

release-ratchet creates commits and tags locally. Your CI pipeline needs to push those back to the remote. That means configuring git auth in CI.

### GitHub Actions

```yaml
# .github/workflows/release.yml
name: Release
on:
  push:
    branches: [main]

jobs:
  release:
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0  # full history for commit analysis

      - name: Install release-ratchet
        run: curl -fsSL https://raw.githubusercontent.com/binary-birthday/release-ratchet/main/install.sh | sh

      - name: Prepare release
        id: prepare
        run: |
          release-ratchet prepare --no-branch || exit 0
          release-ratchet release
          echo "released=true" >> "$GITHUB_OUTPUT"

      - name: Push tag
        if: steps.prepare.outputs.released == 'true'
        run: git push && git push --tags

      - name: Create GitHub release
        if: steps.prepare.outputs.released == 'true'
        run: |
          TAG=$(git describe --tags --abbrev=0)
          release-ratchet notes --latest | gh release create "$TAG" --notes-file -
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

The `GITHUB_TOKEN` is provided automatically by Actions — no setup needed.

### GitLab CI

```yaml
# .gitlab-ci.yml
stages:
  - test
  - release

test:
  stage: test
  image: rust:1.86
  script:
    - cargo clippy --all-targets
    - cargo test

release:
  stage: release
  image: rust:1.86
  rules:
    - if: $CI_COMMIT_BRANCH == "main"
  before_script:
    # Configure git to push back using the CI job token
    - git remote set-url origin "https://gitlab-ci-token:${CI_JOB_TOKEN}@${CI_SERVER_HOST}/${CI_PROJECT_PATH}.git"
    - git config user.name "CI"
    - git config user.email "ci@${CI_SERVER_HOST}"
    - curl -fsSL https://raw.githubusercontent.com/binary-birthday/release-ratchet/main/install.sh | sh
  script:
    - release-ratchet prepare --no-branch || exit 0
    - release-ratchet release
    - git push origin HEAD:main --tags
    - |
      TAG=$(git describe --tags --abbrev=0)
      release-ratchet notes --latest > notes.md
      release-cli create --name "$TAG" --tag-name "$TAG" --description notes.md
```

`CI_JOB_TOKEN` is provided automatically by GitLab — no setup needed.

### Bitbucket Pipelines

```yaml
# bitbucket-pipelines.yml
image: rust:1.86

pipelines:
  branches:
    main:
      - step:
          name: Test
          caches:
            - cargo
          script:
            - cargo clippy --all-targets
            - cargo test

      - step:
          name: Release
          caches:
            - cargo
          script:
            # Configure git to push using an app password
            - git remote set-url origin "https://${BB_USER}:${BB_APP_PASSWORD}@bitbucket.org/${BITBUCKET_REPO_FULL_NAME}.git"
            - git config user.name "CI"
            - git config user.email "ci@bitbucket.org"
            - curl -fsSL https://raw.githubusercontent.com/binary-birthday/release-ratchet/main/install.sh | sh
            - release-ratchet prepare --no-branch || exit 0
            - release-ratchet release
            - git push origin HEAD:main --tags

definitions:
  caches:
    cargo:
      key:
        files:
          - Cargo.lock
      path: target
```

**Bitbucket setup:** Create an [App Password](https://bitbucket.org/account/settings/app-passwords/) with `Repositories: Write` permission. Add `BB_USER` (your username) and `BB_APP_PASSWORD` as [repository variables](https://support.atlassian.com/bitbucket-cloud/docs/variables-and-secrets/) in Bitbucket settings.

### CircleCI

```yaml
# .circleci/config.yml
version: 2.1

orbs:
  rust: circleci/rust@1.6.1

jobs:
  test:
    docker:
      - image: cimg/rust:1.86
    steps:
      - checkout
      - rust/clippy:
          flags: --all-targets
      - rust/test

  release:
    docker:
      - image: cimg/rust:1.86
    steps:
      - checkout
      - run:
          name: Configure git
          command: |
            git config user.name "CI"
            git config user.email "ci@circleci.com"
      - run:
          name: Install and run release-ratchet
          command: |
            curl -fsSL https://raw.githubusercontent.com/binary-birthday/release-ratchet/main/install.sh | sh
            release-ratchet prepare --no-branch || exit 0
            release-ratchet release
            git push origin HEAD:main --tags

workflows:
  build-and-release:
    jobs:
      - test
      - release:
          requires:
            - test
          filters:
            branches:
              only: main
```

**CircleCI setup:** CircleCI's GitHub/Bitbucket integration provides push access via the deploy key added during project setup. No extra tokens needed for pushing commits and tags. For creating GitHub Releases, add a `GITHUB_TOKEN` environment variable in [Project Settings → Environment Variables](https://app.circleci.com/).

### Piping release notes to your forge

```sh
# GitHub
release-ratchet notes --latest | gh release create v1.2.0 --notes-file -

# GitLab
release-ratchet notes --latest | glab release create v1.2.0 --notes -

# Extract notes for a specific version
release-ratchet notes v1.1.0
```

### Consistency checks

Run in CI to catch version drift:

```sh
release-ratchet check
```

Exits 0 if the latest tag matches version files and the changelog has an entry. Exits non-zero otherwise. Use `--json` for machine-readable output.

### Commit message validation

Install a git hook to validate commit messages as you write them:

```sh
release-ratchet hook install
```

Or validate in CI:

```sh
release-ratchet validate --range origin/main..HEAD
```

### Lifecycle hooks

Run commands automatically after prepare or release:

```toml
[hooks]
post_prepare = ["cargo check", "cargo test"]
post_release = ["cargo publish", "npm publish"]
```

Hooks receive a `RELEASE_VERSION` environment variable with the new version.

## Commands

| Command | What it does |
|---|---|
| `prepare` | Reads commits, determines version, updates changelog and version files, creates release commit |
| `release` | Creates a git tag for the release |
| `status` | Shows last release, pending commits, computed next version |
| `notes` | Extracts release notes from changelog (or generates them for unreleased changes) |
| `validate` | Checks commit messages follow conventional commits format |
| `backport` | Cherry-picks commits onto a maintenance branch for hotfix releases |
| `bump` | Updates version files without changelog, commit, or tag (for scripted workflows) |
| `check` | Verifies release consistency (tag matches files, changelog has entry) |
| `hook` | Installs or removes a commit-msg git hook for validation |
| `completions` | Generates shell completion scripts (bash, zsh, fish, powershell) |
| `init` | Creates a `.release-ratchet.toml` config file with defaults |

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Success |
| 1 | Error (unexpected failure) |
| 2 | Nothing to release (no releasable commits) |
| 3 | Validation failed (invalid commit messages or consistency check) |

## License

MIT
