# Jolene

A package manager for coding agent commands, skills, and agents.

Jolene installs packages from git repositories into the config directories
of your AI coding tools — Claude Code, OpenCode, and Codex. Packages are cloned
locally and content is installed via symlinks, so a single `jolene update` pulls
the latest from every package you have installed.

Jolene also works with **Claude Code marketplace repos** — install individual
plugins from existing `.claude-plugin/marketplace.json` catalogs without
requiring upstream changes. Multi-target, symlink-based, CLI-first.

---

## Installation

Pre-built binaries are available on the [GitHub releases page](https://github.com/JuneKelly/jolene/releases).
Download the binary for your platform, make it executable, and place it somewhere on your `PATH`.

Alternatively, build from source with Cargo:

```sh
cargo install --path .
```

---

## Usage

### Install a package

```sh
jolene install --github <owner/repo> [--to <target>...]
jolene install --local  <path>       [--to <target>...]
jolene install --url    <git-url>    [--to <target>...]
```

Exactly one of `--github`, `--local`, or `--url` is required. Clones the
repository and creates symlinks for all supported content. If `--to` is
omitted, jolene installs to every target whose config directory exists on
your system.

```
$ jolene install --github JuneKelly/co-review --to claude-code
Installing JuneKelly/co-review...
  Cloning https://github.com/JuneKelly/co-review.git
  Found: 1 command

  Installing to claude-code:
    + commands/co-review.md -> ~/.claude/commands/co-review.md

Installed JuneKelly/co-review to claude-code
```

### Install from a Claude Code marketplace

```sh
jolene install --marketplace --github <org/repo> --pick <plugin>[,<plugin>...] [--to <target>...]
```

Install individual plugins from a Claude Code marketplace repo. The repo must
contain `.claude-plugin/marketplace.json`. Use `jolene contents` to browse
available plugins first.

```
$ jolene install --marketplace --github acme-corp/tools --pick review-plugin
Installing from marketplace acme-corp/tools...
  Cloning https://github.com/acme-corp/tools.git
  Marketplace: acme-tools

  Plugin: review-plugin
    Code review skill for PRs
    Found: 1 skill, 1 command

  Installing to claude-code:
    + skills/review -> ~/.claude/skills/review
    + commands/quick-review.md -> ~/.claude/commands/quick-review.md

  Installed plugin 'review-plugin' to claude-code
```

Jolene installs commands, skills, and agents from plugins. Hooks, MCP servers,
and LSP servers are Claude Code-specific and are skipped with a warning.

### Browse contents

```sh
jolene contents --marketplace --github <org/repo>    # browse a marketplace
jolene contents <installed-package>                   # inspect an installed package
jolene contents --github <owner/repo>                 # inspect a native package
```

```
$ jolene contents --marketplace --github acme-corp/tools
acme-tools
  Enterprise workflow tools
  Maintained by: DevTools Team

Available plugins (3):

  review-plugin            Code review skill for PRs
  deploy-tools             Deployment automation commands
  security-scanner         Security analysis agent

Install with: jolene install --marketplace --github acme-corp/tools --pick <plugin>
```

### List installed packages

```sh
jolene list [--target <target>]
```

```
Installed packages:

  JuneKelly/co-review
    Source:  github
    Targets: opencode, claude-code
    Content: 1 command, 2 skills
    Version: (main@abc1234)
```

### Update packages

```sh
jolene update [<package>]
```

Pulls the latest commits, adds symlinks for new content, and removes symlinks
for deleted content. Omit `<package>` to update everything.

### Show package details

```sh
jolene info <package>
```

### Uninstall a package

```sh
jolene uninstall <package> [--from <target>...] [--purge]
```

Removes all symlinks for the package. `--purge` also deletes the cloned
repository from the local store.

### Check installation health

```sh
jolene doctor
```

Verifies all recorded symlinks exist and resolve correctly. Reports broken
symlinks, missing clones, and orphaned symlinks.

---

## Supported Targets

| Target      | Slug          | Config Root               |
|-------------|---------------|---------------------------|
| Claude Code | `claude-code` | `~/.claude/`              |
| OpenCode    | `opencode`    | `~/.config/opencode/`     |
| Codex       | `codex`       | `~/.codex/`               |

Targets are auto-detected by checking whether their config root exists. Use
`--to` / `--from` to override.

---

## Package Format

### Native packages

A jolene package is a git repository with a `jolene.toml` manifest and
content in conventional directories.

### Directory structure

```
repo-root/
  jolene.toml
  commands/        # .md files, one per command
    review.md
  skills/          # one subdirectory per skill, must contain SKILL.md
    code-analysis/
      SKILL.md
  agents/          # .md files, one per agent
    reviewer.md
```

### jolene.toml

```toml
[package]
name = "review-tools"
description = "Code review commands and analysis skills"
version = "1.0.0"
authors = ["junebug <junebug@example.com>"]
license = "MIT"

[package.urls]
repository = "https://github.com/junebug/review-tools"

[content]
commands = ["review"]
skills = ["code-analysis"]
agents = ["reviewer"]
```

Only items declared in `[content]` are installed. At least one item must be
declared. All three content lists are optional, but at least one must be
non-empty.

### Content type support by target

| Content type | Claude Code | OpenCode | Codex |
|--------------|:-----------:|:--------:|:-----:|
| Commands     | yes         | yes      | —     |
| Skills       | yes         | yes      | yes   |
| Agents       | yes         | yes      | —     |

Unsupported content types are silently skipped (visible with `--verbose`).

### Marketplace packages

Claude Code marketplace repos use `.claude-plugin/marketplace.json` to catalog
plugins. Each plugin has its own directory with the same `commands/`, `skills/`,
`agents/` layout. Jolene discovers content by scanning the filesystem — no
`jolene.toml` needed in plugin directories.

Plugins can live inside the marketplace repo (relative source) or in their own
repos (GitHub/URL source). See `docs/SPEC.md` for the full marketplace format.

---

## Global flags

```
-v, --verbose    Print detailed output
-q, --quiet      Suppress non-error output
-V, --version    Print version
-h, --help       Print help
```

---

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `JOLENE_ROOT` | `~/.jolene/` | Override the jolene data directory (store, state file). |
| `JOLENE_EFFECTIVE_HOME` | `$HOME` | Override the home directory used for target config paths (`~/.claude/`, etc.) and `~/...` display/expansion. |

These are primarily useful for integration testing — point both variables at
temp directories to run jolene without touching real config files.
