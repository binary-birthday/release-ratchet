# release-ratchet

Git-vendor-agnostic semantic release tool using conventional commits.

## Install

### Pre-built binary (Linux x86_64)

```sh
curl -fsSL https://raw.githubusercontent.com/binary-birthday/release-ratchet/main/install.sh | sh
```

### Build from source (macOS / Linux / Windows)

Requires [Rust](https://rustup.rs/) 1.86+.

```sh
git clone https://github.com/binary-birthday/release-ratchet.git
cd release-ratchet
cargo build --release
```

The binary is at `target/release/release-ratchet`. Copy it somewhere on your PATH:

```sh
cp target/release/release-ratchet /usr/local/bin/
```

### macOS notes

On macOS, the build requires the Xcode Command Line Tools for the C dependencies (libgit2, openssl):

```sh
xcode-select --install
```

If you hit OpenSSL linking errors, install it via Homebrew and set the env vars:

```sh
brew install openssl pkg-config
export OPENSSL_DIR=$(brew --prefix openssl)
cargo build --release
```

### Verify

```sh
release-ratchet --version
release-ratchet --help
```

## Quick start

```sh
# Initialize config
release-ratchet init

# Check what would be released
release-ratchet status

# Prepare a release (changelog, version bump, release commit)
release-ratchet prepare --no-branch

# Tag it
release-ratchet release

# Extract notes for a GitHub release
release-ratchet notes --latest | gh release create v0.1.0 --notes-file -
```

## Commands

| Command | Purpose |
|---|---|
| `prepare` | Analyze commits, bump version, generate changelog, create release commit |
| `release` | Tag the release commit |
| `status` | Show last release, pending commits, next version |
| `validate` | Validate commit messages against conventional commits |
| `notes` | Extract or generate release notes |
| `backport` | Cherry-pick fixes to maintenance branches |
| `bump` | Bump version files only (no changelog/commit/tag) |
| `check` | Verify tag/file/changelog consistency |
| `hook` | Install/uninstall commit-msg validation hook |
| `completions` | Generate shell completions |
| `init` | Create default config |

## Config

Config lives in `.release-ratchet.toml`:

```toml
tag_prefix = "v"
main_branch = "main"
release_branch = "release-ratchet--release"
changelog_path = "CHANGELOG.md"

[[ecosystems]]
type = "cargo"
path = "Cargo.toml"
```

If no config file exists and no ecosystems are configured, release-ratchet auto-detects `Cargo.toml`, `package.json`, and `pyproject.toml`.

### Monorepo

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

[[shared_paths]]
path = "utils"
affects = ["core", "cli"]
```

Use `-p <name>` to target a single package: `release-ratchet -p core status`.

## License

MIT
