# Jolene

A package manager for coding agent commands, skills, and agents.

Jolene installs packages from git repositories into the config directories
of your AI coding tools — Claude Code, OpenCode, and Codex. Packages are cloned
locally and content is installed via symlinks, so a single `jolene update` pulls
the latest from every package you have installed.

---

## Installation

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
$ jolene install --github junebug/review-tools --to opencode
Installing junebug/review-tools...
  Cloning https://github.com/junebug/review-tools.git
  Found: 1 command, 2 skills

  Installing to opencode:
    + commands/review.md -> ~/.config/opencode/commands/review.md
    + skills/code-analysis/ -> ~/.config/opencode/skills/code-analysis/
    + skills/style-check/ -> ~/.config/opencode/skills/style-check/

Installed junebug/review-tools to opencode
```

### List installed packages

```sh
jolene list [--target <target>]
```

```
Installed packages:

  junebug/review-tools
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

---

## Global flags

```
-v, --verbose    Print detailed output
-q, --quiet      Suppress non-error output
-V, --version    Print version
-h, --help       Print help
```
