# Jolene

A plugin manager for coding agent commands, skills, and agents.

Jolene installs bundles from git repositories into the config directories
of your AI coding tools — Claude Code, OpenCode, and Codex. Bundles are cloned
locally and content is installed via symlinks, so a single `jolene update` pulls
the latest from every bundle you have installed.

Jolene also works with **Claude Code marketplace repos** — install individual
plugins from existing `.claude-plugin/marketplace.json` catalogs without
requiring upstream changes. Multi-target, symlink-based, CLI-first.

---

## Installation

Pre-built binaries are available on the [GitHub releases page](https://github.com/JuneKelly/jolene/releases).
Download the binary for your platform, make it executable, and place it somewhere on your `PATH`.

Alternatively, build from source:

```sh
just install
```

This requires [just](https://github.com/just-systems/just) and a Rust toolchain.

---

## Usage

### Install a bundle

```sh
jolene install --github <owner/repo> [--to <target>...] [--prefix <value> | --no-prefix] [--var key=value...] [--vars-json '{...}'...]
jolene install --local  <path>       [--to <target>...] [--prefix <value> | --no-prefix] [--var key=value...] [--vars-json '{...}'...]
jolene install --url    <git-url>    [--to <target>...] [--prefix <value> | --no-prefix] [--var key=value...] [--vars-json '{...}'...]
```

Exactly one of `--github`, `--local`, or `--url` is required. Clones the
repository and creates symlinks for all supported content. If `--to` is
omitted, jolene installs to every target whose config directory exists on
your system.

Use `--prefix <value>` to namespace installed content and avoid name collisions:

```
$ jolene install --github JuneKelly/co-review --prefix jk --to claude-code
Installing JuneKelly/co-review...
  Cloning https://github.com/JuneKelly/co-review.git
  Found: 1 command
  Prefix: jk

  Installing to claude-code:
    + commands/co-review.md -> ~/.claude/commands/jk--co-review.md

Installed JuneKelly/co-review to claude-code
```

Use `--no-prefix` to suppress a manifest-defined prefix and install flat.
Bundle authors can set a default prefix in `jolene.toml` with `prefix = "abc"`
in the `[bundle]` table.

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
jolene install --marketplace --github <org/repo> --pick <plugin>[,<plugin>...] [--to <target>...] [--prefix <value> | --no-prefix]
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

During install, jolene checks each skill's `SKILL.md` frontmatter and warns
about missing `name` or `description` fields, displays `compatibility` notes,
and flags non-executable scripts. These are advisory only and never block
installation.

### Browse contents

```sh
jolene contents --marketplace --github <org/repo>    # browse a marketplace
jolene contents <installed-bundle>                   # inspect an installed bundle
jolene contents --github <owner/repo>                 # inspect a native bundle
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

### List installed bundles

```sh
jolene list [--target <target>]
```

```
Installed bundles:

  JuneKelly/co-review
    Source:  github
    Targets: opencode, claude-code
    Content: 1 command, 2 skills
    Version: (main@abc1234)
```

### Update bundles

```sh
jolene update [<bundle>]
```

Pulls the latest commits, adds symlinks for new content, and removes symlinks
for deleted content. Omit `<bundle>` to update everything.

### Show bundle details

```sh
jolene info <bundle>
```

### Uninstall a bundle

```sh
jolene uninstall <bundle> [--from <target>...] [--purge]
```

Removes all symlinks for the bundle. `--purge` also deletes the cloned
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

## Bundle Format

### Native bundles

A jolene bundle is a git repository with a `jolene.toml` manifest and
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
[bundle]
name = "review-tools"
description = "Code review commands and analysis skills"
version = "1.0.0"
authors = ["junebug <junebug@example.com>"]
license = "MIT"
prefix = "jb"    # optional — default prefix for installed content names

[bundle.urls]
repository = "https://github.com/junebug/review-tools"

[content]
commands = ["review"]
skills = ["code-analysis"]
agents = ["reviewer"]

[template.vars]                    # optional — template variables
doc_url       = "https://example.com/docs"
show_advanced = false
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

### Templating

Content files may embed template expressions that are evaluated at install
time. This enables correct cross-references between content items (e.g. a
skill that references a companion command by its installed name, including
prefix) and target-conditional content.

Template expressions use custom delimiters (`{~ ~}` for values, `{%~ ~%}` for
blocks, `{#~ ~#}` for comments) chosen to avoid collisions with other template
systems. Everything is namespaced under `jolene`:

```text
Run /{~ jolene.resolve("deploy") ~} to deploy.

{%~ if jolene.target == "claude-code" ~%}
Use the slash command: `/{~ jolene.resolve("deploy") ~}`.
{%~ endif ~%}

API docs: {~ jolene.vars.doc_url ~}
```

Available context:
- `jolene.resolve("name")` — installed name of a content item, with prefix applied
- `jolene.prefix` — the active prefix, or `""` if none
- `jolene.target` — target slug (`"claude-code"`, `"opencode"`, `"codex"`)
- `jolene.bundle.name` / `jolene.bundle.version` — from manifest
- `jolene.vars.*` — variables declared in `[template.vars]`

Override template variables at install time:

```sh
jolene install --github foo/bar \
  --var doc_url=https://internal.corp/docs \
  --var show_advanced=true \
  --vars-json '{"notify_channels": ["slack", "pagerduty"]}'
```

Variable overrides are stored in the state file and preserved across
`jolene update`. Templating applies to native bundles only — marketplace
content is not processed.

See [docs/TEMPLATING.md](docs/TEMPLATING.md) for the full templating guide,
and [docs/SPEC.md](docs/SPEC.md) Section 7 for the specification.

### Marketplace plugins

Claude Code marketplace repos use `.claude-plugin/marketplace.json` to catalog
plugins. Each plugin has its own directory with the same `commands/`, `skills/`,
`agents/` layout. Jolene discovers content by scanning the filesystem — no
`jolene.toml` needed in plugin directories.

Plugins can live inside the marketplace repo (relative source) or in their own
repos (GitHub/URL source). See `docs/SPEC.md` for the full marketplace format.

---

## Development

This project uses [just](https://github.com/just-systems/just) as a task runner.

```sh
just          # list available recipes
just build    # build the project
just test     # run tests
just install  # install jolene to ~/.cargo/bin
```

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
