# Jolene Specification

A package manager for coding agent commands, skills, and agents.

## Overview

Jolene installs packages from git repositories that bundle commands, skills,
and agents for coding AI tools. Packages are cloned into a local store and
installed to target tool directories using symlinks.

### Source Types

Jolene supports three install source types:

| Flag        | Argument         | Description                                      |
|-------------|------------------|--------------------------------------------------|
| `--github`  | `owner/repo`     | GitHub repository (shorthand for the HTTPS URL). |
| `--local`   | `./path`         | Local git repository, cloned into the store.     |
| `--url`     | `https://...`    | Arbitrary remote git URL.                        |

### Supported Targets

| Target      | Config Root              | Slug          |
|-------------|--------------------------|---------------|
| Claude Code | `~/.claude/`             | `claude-code` |
| OpenCode    | `~/.config/opencode/`    | `opencode`    |
| Codex       | `~/.codex/`              | `codex`       |

### Content Types

| Content Type | Package Directory | Claude Code            | OpenCode                        | Codex                |
|--------------|-------------------|------------------------|---------------------------------|----------------------|
| Commands     | `commands/*.md`   | `~/.claude/commands/`  | `~/.config/opencode/commands/`  | not supported        |
| Skills       | `skills/*/`       | `~/.claude/skills/`    | `~/.config/opencode/skills/`    | `~/.codex/skills/`   |
| Agents       | `agents/*.md`     | `~/.claude/agents/`    | `~/.config/opencode/agents/`    | not supported        |

When a content type is not supported by a target, jolene skips it silently.
In verbose mode, it prints a notice.

---

## 1. CLI Interface

### Global Flags

```
--verbose, -v    Print detailed output
--quiet, -q      Suppress non-error output
--help, -h       Print help
--version, -V    Print version
```

### jolene install

#### Native mode (default)

```
jolene install --github <owner/repo> [--to <target>...]
jolene install --local  <path>       [--to <target>...]
jolene install --url    <git-url>    [--to <target>...]
```

Exactly one of `--github`, `--local`, or `--url` is required.

- `--github <owner/repo>` — GitHub repository. Expands to `https://github.com/{owner}/{repo}.git`.
- `--local <path>` — Local git repository. Cloned into the store; the original is not modified.
- `--url <git-url>` — Arbitrary remote git URL.
- `--to <target>` — Target slug(s). Repeatable. If omitted, installs to all
  targets whose config root directory exists on the system.

**Process:**

1. Clone (or pull if already cloned) the repo into the local store.
2. Validate: repo must have `jolene.toml` and at least one content directory.
3. For each target, create symlinks for all supported content types.
4. Record installation in the state file.

#### Marketplace mode (`--marketplace`)

```
jolene install --marketplace --github <org/repo> --pick <plugin>[,<plugin>...] [--to <target>...]
```

- `--marketplace` — Treat the source as a Claude Code marketplace repository
  (expects `.claude-plugin/marketplace.json`).
- `--pick <name>` — Select plugins from the marketplace catalog. Comma-separated.
  Required when `--marketplace` is set.

**Process:**

1. Clone (or pull) the marketplace repo.
2. Parse `.claude-plugin/marketplace.json`.
3. For each picked plugin, resolve its source:
   - **Relative** (`./plugins/foo`): content lives within the marketplace clone.
   - **GitHub** (`{ "source": "github", "repo": "owner/repo" }`): clone independently.
   - **URL** (`{ "source": "url", "url": "https://..." }`): clone independently.
   - **npm/pip**: error — not yet supported.
4. Discover content by scanning `commands/*.md`, `skills/*/SKILL.md`, `agents/*.md`.
5. Warn if the plugin has hooks, MCP servers, or LSP servers (jolene does not install these).
6. Create symlinks and record state, same as native mode.

**What jolene ignores from plugins:** Hooks (`hooks/hooks.json`), MCP servers
(`.mcp.json`), LSP servers (`.lsp.json`), and plugin settings (`settings.json`).
These are Claude Code-specific features with no equivalent in other targets.

**Example (GitHub):**

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

**Example (local):**

```
$ jolene install --local ./my-tools
Installing /Users/you/projects/my-tools...
  Cloning /Users/you/projects/my-tools
  Found: 2 commands

  Installing to claude-code:
    + commands/foo.md -> ~/.claude/commands/foo.md
    + commands/bar.md -> ~/.claude/commands/bar.md

Installed /Users/you/projects/my-tools to claude-code
```

**Example (marketplace):**

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

### jolene uninstall

```
jolene uninstall <package> [--from <target>...] [--purge]
```

- `<package>` — `Author/repo` or just `repo` if unambiguous.
- `--from <target>` — Remove from specific targets. If omitted, removes from all.
- `--purge` — Also delete the cloned repo from the local store.

### jolene list

```
jolene list [--target <target>]
```

**Example output:**

```
Installed packages:

  junebug/review-tools
    Source:  github
    Targets: opencode, claude-code
    Content: 1 command, 2 skills
    Version: (main@abc1234)

  /Users/you/projects/my-tools
    Source:  local
    Targets: claude-code
    Content: 2 commands
    Version: (main@def5678)

  https://gitlab.com/someone/cool-tools.git
    Source:  url
    Targets: claude-code
    Content: 3 skills
    Version: (main@fed9876)
```

### jolene update

```
jolene update [<package>]
```

Updates one or all packages by pulling the latest from the default branch.
Creates symlinks for new content, removes symlinks for deleted content,
and updates the state file.

### jolene info

```
jolene info <package>
```

Shows detailed information about an installed package including source URL,
installed targets, branch, commit, and all content items.

### jolene contents

Browse the contents of a marketplace or installed package before installing.

```
jolene contents --marketplace --github <org/repo>    # remote marketplace
jolene contents <installed-package>                   # installed package
jolene contents --github <owner/repo>                 # remote native package
```

**Example (marketplace):**

```
$ jolene contents --marketplace --github acme-corp/tools
acme-tools
  Enterprise workflow tools
  Maintained by: DevTools Team

Available plugins (4):

  review-plugin            Code review skill for PRs
  deploy-tools             Deployment automation commands
  security-scanner         Security analysis agent
  code-formatter           Auto-formatting on save (hooks only — not installable by jolene)

Install with: jolene install --marketplace --github acme-corp/tools --pick <plugin>
```

Plugins that contain only hooks/MCP/LSP (no commands, skills, or agents) are
flagged as "not installable by jolene."

**Example (installed package):**

```
$ jolene contents review-plugin
acme-corp/tools::review-plugin
  From marketplace: acme-corp/tools
  Plugin: review-plugin

  Skills:
    review
  Commands:
    quick-review
```

### jolene doctor

```
jolene doctor
```

Verifies health of all installations:
- Checks all recorded symlinks exist and resolve to valid targets.
- Reports broken symlinks, missing clones, and orphaned symlinks.

---

## 2. Package Format

A jolene package is a git repository with a `jolene.toml` manifest and
content organized in conventional directories.

### Directory Structure

```
repo-root/
  jolene.toml             # required manifest
  commands/               # command files (.md with YAML frontmatter)
    review.md
    deploy.md
  skills/                 # skill directories (each with SKILL.md)
    code-analysis/
      SKILL.md
      references/
        patterns.md
  agents/                 # agent definitions (.md with YAML frontmatter)
    reviewer.md
```

A package MUST have a `jolene.toml` and at least one content directory
(`commands/`, `skills/`, or `agents/`) with at least one item inside.

### Content Rules

- `commands/*.md` — Only `.md` files at the top level. Subdirectories ignored.
- `skills/*/` — Each subdirectory is a skill. Must contain `SKILL.md`.
  Additional files within the skill directory are preserved.
- `agents/*.md` — Only `.md` files at the top level. Subdirectories ignored.

### Manifest: jolene.toml

```toml
[package]
name = "review-tools"
description = "Code review commands and analysis skills"
version = "1.0.0"
authors = ["junebug <junebug@example.com>"]
license = "MIT"

[package.urls]
repository = "https://github.com/junebug/review-tools"
homepage = "https://junebug.dev/review-tools"    # optional

[content]
commands = ["review", "deploy"]
skills = ["code-analysis", "style-check"]
agents = ["reviewer"]
```

**Required fields:**

| Field         | Type       | Description                              |
|---------------|------------|------------------------------------------|
| `name`        | string     | Package name. Must match `[a-z0-9-]+`.   |
| `description` | string     | One-line description of the package.      |
| `version`     | string     | Semantic version (e.g. `1.0.0`).         |
| `authors`     | string[]   | List of authors. Format: `"Name <email>"` or `"Name"`. |
| `license`     | string     | SPDX license identifier (e.g. `MIT`, `Apache-2.0`). |

**Required content declaration:**

The `[content]` table declares exactly which items the package provides.
Only declared items are installed — files in content directories that aren't
listed in the manifest are ignored. At least one item must be declared.

| Field      | Type     | Description                                     |
|------------|----------|-------------------------------------------------|
| `commands` | string[] | Command names (without `.md`). Maps to `commands/{name}.md`. |
| `skills`   | string[] | Skill directory names. Maps to `skills/{name}/`. |
| `agents`   | string[] | Agent names (without `.md`). Maps to `agents/{name}.md`. |

All three fields are optional, but at least one must be present and non-empty.

**Optional fields:**

| Field                  | Type   | Description                        |
|------------------------|--------|------------------------------------|
| `package.urls.repository` | string | Source repository URL.          |
| `package.urls.homepage`   | string | Project homepage or docs URL.   |

### Marketplace Format (Claude Code Plugin Repos)

Claude Code marketplace repos use `.claude-plugin/marketplace.json` instead of
`jolene.toml`. Jolene can install content from these repos using `--marketplace`
mode, without requiring upstream changes.

#### marketplace.json

```json
{
  "name": "acme-tools",
  "owner": { "name": "DevTools Team" },
  "metadata": { "description": "Enterprise workflow tools" },
  "plugins": [
    {
      "name": "review-plugin",
      "source": "relative",
      "path": "./plugins/review-plugin",
      "description": "Code review skill for PRs"
    },
    {
      "name": "deploy-tools",
      "source": "github",
      "repo": "acme-corp/deploy-tools",
      "description": "Deployment automation commands"
    },
    {
      "name": "scanner",
      "source": "url",
      "url": "https://gitlab.com/acme/scanner.git",
      "description": "Security analysis agent"
    }
  ]
}
```

#### Plugin source types

| Source       | Resolution                                              |
|--------------|---------------------------------------------------------|
| `relative`   | Subdirectory within the marketplace repo. No extra clone. |
| `github`     | Independent clone into jolene's store (`repos/{hash}/`).  |
| `url`        | Independent clone into jolene's store (`repos/{hash}/`).  |
| `npm`, `pip` | Not supported — error with message.                       |

#### Content discovery

Marketplace plugins do not need a `jolene.toml`. Content is discovered by
scanning the plugin directory:

- `commands/*.md` → Command items
- `skills/*/SKILL.md` → Skill items (SKILL.md must exist)
- `agents/*.md` → Agent items

This is the same directory layout used by native jolene packages and by Claude
Code plugins. The only difference is the discovery mechanism (filesystem scan
vs. manifest declaration).

#### Ignored features

Jolene installs only commands, skills, and agents. These Claude Code-specific
features are detected and warned about but not installed:

- Hooks (`hooks/hooks.json` or declared in `plugin.json`)
- MCP servers (`.mcp.json` or declared in `plugin.json`)
- LSP servers (`.lsp.json` or declared in `plugin.json`)

---

## 3. Local Store

All jolene data lives under `~/.jolene/`.

### Directory Layout

```
~/.jolene/
  state.toml                        # installation state
  repos/                            # cloned repositories
    a3f2c1d8e9b4f761a0b5c3d2e8f4a7b1c9d6e2f5a8b3c7d1e4f9a2b6c0d5e3f8/  # git clone
    b8e1d4f9a2c7e0b3d5f2a9c4e7b1d3f6a8c2e5b9d4f7a1c3e8b6d2f4a0c9e3b7/  # git clone
```

Each directory under `repos/` is named with the SHA256 of the package's canonical key
(see Store key below). `state.toml` is the authoritative mapping from hash to source.

### State File: state.toml

Tracks installed packages and their symlinks.

```toml
[[packages]]
source_kind  = "github"
source       = "junebug/review-tools"
clone_url    = "https://github.com/junebug/review-tools.git"
clone_path   = "repos/a3f2c1d8e9b4f761a0b5c3d2e8f4a7b1c9d6e2f5a8b3c7d1e4f9a2b6c0d5e3f8"
branch      = "main"
commit      = "abc1234def5678"
installed_at = "2026-02-28T10:00:00Z"
updated_at   = "2026-02-28T10:00:00Z"

  [[packages.installations]]
  target = "opencode"
  symlinks = [
    { src = "commands/review.md", dst = "~/.config/opencode/commands/review.md" },
    { src = "skills/code-analysis", dst = "~/.config/opencode/skills/code-analysis" },
    { src = "skills/style-check", dst = "~/.config/opencode/skills/style-check" },
  ]

  [[packages.installations]]
  target = "claude-code"
  symlinks = [
    { src = "commands/review.md", dst = "~/.claude/commands/review.md" },
    { src = "skills/code-analysis", dst = "~/.claude/skills/code-analysis" },
    { src = "skills/style-check", dst = "~/.claude/skills/style-check" },
  ]
```

**Path conventions:**
- `src` paths are relative to the package clone root (or plugin subdirectory for relative marketplace plugins).
- `dst` paths use `~` for home directory (expanded at runtime).
- `clone_path` is relative to `~/.jolene/` and always has the form `repos/{64-char-hex}`.

**Source fields:**

| Field         | Description                                                    |
|---------------|----------------------------------------------------------------|
| `source_kind` | `"github"` \| `"local"` \| `"url"`. Defaults to `"github"` for pre-existing entries. |
| `source`      | Human-readable identifier. For native packages: `owner/repo`, absolute path, or URL. For relative marketplace plugins: `owner/marketplace::plugin-name`. Used for display and lookup. |
| `clone_url`   | The git URL used to clone the package. Absent for pre-existing entries. |
| `clone_path`  | `repos/{64-char-hex}` — relative to `~/.jolene/`. The hex is the SHA256 store key. |

**Marketplace provenance fields** (optional, present only for marketplace-sourced packages):

| Field         | Description                                                    |
|---------------|----------------------------------------------------------------|
| `marketplace` | The marketplace source identifier (e.g. `"acme-corp/tools"`). |
| `plugin_name` | The plugin name within the marketplace (e.g. `"review-plugin"`). Enables short-name lookup: `jolene update review-plugin`. |
| `plugin_path` | For relative plugins, the subdirectory within the clone where content lives (e.g. `"plugins/review-plugin"`). Absent for external plugins. |

**Store key for marketplace plugins:**
- **Relative-path plugins** share the marketplace repo's store key. Multiple
  relative plugins from the same marketplace share one clone directory but get
  distinct `PackageState` entries (distinguished by `source` which includes the
  `::plugin-name` suffix).
- **External-source plugins** (GitHub/URL) get their own store key and clone,
  just like any other jolene package. The marketplace merely told us about them.

**Store key:** Each package is identified by the SHA256 hex digest of its canonical key string:
- GitHub: SHA256 of `github||owner/repo`
- Local:  SHA256 of `local||/absolute/path`
- URL:    SHA256 of `url||https://...`

The 64-character hex digest is used as the directory name under `repos/`.
`state.toml` is the authoritative mapping from hash to human-readable source.

**Example (marketplace-sourced relative plugin):**

```toml
[[packages]]
source_kind  = "github"
source       = "acme-corp/tools::review-plugin"
clone_url    = "https://github.com/acme-corp/tools.git"
clone_path   = "repos/b8e1d4f9a2c7e0b3d5f2a9c4e7b1d3f6a8c2e5b9d4f7a1c3e8b6d2f4a0c9e3b7"
branch       = "main"
commit       = "fed9876abc1234"
installed_at = "2026-03-02T10:00:00Z"
updated_at   = "2026-03-02T10:00:00Z"
marketplace  = "acme-corp/tools"
plugin_name  = "review-plugin"
plugin_path  = "plugins/review-plugin"

  [[packages.installations]]
  target = "claude-code"
  symlinks = [
    { src = "skills/review", dst = "~/.claude/skills/review" },
    { src = "commands/quick-review.md", dst = "~/.claude/commands/quick-review.md" },
  ]
```

**Atomicity:** The state file is written to a temp file then renamed,
preventing corruption on interruption.

---

## 4. Installation Process

### Install: Step by Step

```
1. RESOLVE SOURCE
   --github owner/repo → clone URL:    https://github.com/{owner}/{repo}.git
                         canonical key: github||owner/repo
                         store key:    SHA256(canonical key) — 64-char hex
   --local  ./path     → clone URL:    absolute path (git supports local clones)
                         canonical key: local||/absolute/path
                         store key:    SHA256(canonical key) — 64-char hex
   --url    https://…  → clone URL:    the URL as-is
                         canonical key: url||https://...
                         store key:    SHA256(canonical key) — 64-char hex

2. FETCH
   - Exists in store? → git pull
   - New? → git clone into ~/.jolene/repos/{store-key}/

3. VALIDATE
   - jolene.toml must exist and parse correctly (all required fields).
   - [content] must declare at least one item.
   - Each declared item must exist on disk:
     commands/{name}.md, skills/{name}/SKILL.md, agents/{name}.md.
   - Abort with descriptive error if validation fails.

4. RESOLVE TARGETS
   - If --to specified: use those targets.
   - If --to omitted: detect by checking which config roots exist.
   - If none found: error with guidance to use --to.

5. CHECK CONFLICTS (per target, per content item)
   - Destination is a symlink into ~/.jolene/ from a different package:
     → Package conflict. Abort with message naming both packages.
   - Destination is a symlink into ~/.jolene/ from the same package:
     → Reinstall. Skip (already correct).
   - Destination exists but is not a jolene-managed symlink:
     → User conflict. Abort, ask user to remove/rename.
   - Destination does not exist:
     → Proceed.

6. CREATE DIRECTORIES
   Ensure target content directories exist (mkdir -p).

7. CREATE SYMLINKS
   - Commands/Agents: symlink each .md file individually.
   - Skills: symlink each skill directory.
   - All symlinks use absolute paths (no ~, fully expanded).

8. RECORD STATE
   Write updated state.toml (atomic write).
```

### Marketplace Install: Step by Step

```
1. RESOLVE SOURCE
   Same as native mode: --github, --local, or --url.

2. FETCH MARKETPLACE
   Clone (or pull) the marketplace repo into the store.

3. PARSE CATALOG
   Read .claude-plugin/marketplace.json. Error if missing.
   Validate that --pick names exist in the catalog.

4. FOR EACH PICKED PLUGIN:

   4a. RESOLVE PLUGIN SOURCE
       - Relative: resolve to subdirectory within the marketplace clone.
       - GitHub/URL: clone independently into its own store directory.
       - npm/pip: error (not supported).

   4b. DETECT IGNORED FEATURES
       Check for hooks.json, .mcp.json, .lsp.json.
       Warn user if present.

   4c. DISCOVER CONTENT
       Scan plugin directory for commands/*.md, skills/*/SKILL.md, agents/*.md.
       Skip plugin if no installable content found.

   4d. RESOLVE TARGETS, CHECK CONFLICTS, CREATE SYMLINKS
       Same as native install (steps 4-7 above).

   4e. RECORD STATE
       Store with marketplace provenance fields.
       Relative plugins use composite source: "org/marketplace::plugin-name".
       External plugins use their own source identity.
```

### Package Name Lookup

Packages can be referenced by short name in `uninstall`, `update`, `info`,
and `contents` commands:

- **Native packages:** `"tools"` matches `"alice/tools"` (the repo component).
- **Marketplace plugins:** `"review-plugin"` matches any package with
  `plugin_name = "review-plugin"`.
- If multiple packages match a short name, jolene errors with "Ambiguous name"
  and lists the matches.

### Rollback

If any symlink creation fails (e.g., conflict on the third file), jolene
removes all symlinks it created during this operation and does not update
the state file. This ensures state always reflects reality.

### Uninstall: Step by Step

```
1. LOOKUP package in state.toml. Match by Author/repo or repo (error if ambiguous).
2. SCOPE to --from targets, or all targets if omitted.
3. REMOVE symlinks. Warn (don't error) if a symlink is already gone.
4. UPDATE state.toml. Remove target entries; remove package if no targets remain.
5. PURGE clone if --purge flag set.
```

### Update: Step by Step

```
1. git pull in the clone directory.
2. Diff content: compare current files against recorded symlinks.
3. Create symlinks for new content.
4. Remove symlinks for deleted content.
5. Update commit hash and timestamp in state.toml.
```

---

## 5. Target Adapters

Each target defines its config root, content subdirectories, and supported
content types.

```
claude-code:
  root:     ~/.claude/
  commands: commands/     (supported)
  skills:   skills/       (supported)
  agents:   agents/       (supported)
  detect:   ~/.claude/ exists

opencode:
  root:     ~/.config/opencode/
  commands: commands/     (supported)
  skills:   skills/       (supported)
  agents:   agents/       (supported)
  detect:   ~/.config/opencode/ exists

codex:
  root:     ~/.codex/
  commands: —             (not supported)
  skills:   skills/       (supported)
  agents:   —             (not supported)
  detect:   ~/.codex/ exists
```

### Path Resolution Example

Package content `commands/review.md` installed to `opencode`:

```
symlink_source (absolute): /Users/you/.jolene/repos/a3f2c1d8e9b4f761a0b5c3d2e8f4a7b1c9d6e2f5a8b3c7d1e4f9a2b6c0d5e3f8/commands/review.md
symlink_target:            /Users/you/.config/opencode/commands/review.md
```

### Unsupported Content

When installing to a target that doesn't support a content type, jolene skips
silently. With `--verbose`:

```
  Skipping 1 command for codex (not supported)
  Skipping 1 agent for codex (not supported)
```

---

## 6. Symlink Strategy

### Why Symlinks

- **Auto-update:** `git pull` in the store updates all installed content with
  no re-copy step.
- **Traceability:** `readlink` shows exactly which package provides a file.
- **Proven:** Claude Code already works with symlinked commands.
- **Efficient:** No file duplication.

### File-Level Symlinks (Commands, Agents)

Each `.md` file gets its own symlink:

```
~/.claude/commands/review.md → /home/user/.jolene/repos/a3f2c1d8e9b4f761a0b5c3d2e8f4a7b1c9d6e2f5a8b3c7d1e4f9a2b6c0d5e3f8/commands/review.md
```

### Directory-Level Symlinks (Skills)

Each skill directory is symlinked whole to preserve internal structure:

```
~/.claude/skills/code-analysis/ → /home/user/.jolene/repos/a3f2c1d8e9b4f761a0b5c3d2e8f4a7b1c9d6e2f5a8b3c7d1e4f9a2b6c0d5e3f8/skills/code-analysis/
```

### No Namespace Prefix

Symlinks use the original filename from the package. `/review` stays `/review`,
not `/junebug-review`. This keeps daily usage clean. Conflicts are handled
explicitly (see Section 7).

### Absolute Paths

All symlinks use fully expanded absolute paths. No `~` or relative paths.
This avoids breakage from shell or working directory context.

---

## 7. Error Handling

### Local Path Not Found

```
Error: Cannot access local path: ./nonexistent
```

### Package Not Found

```
Error: Failed to clone https://github.com/nonexistent/repo.git
  Repository not found or not accessible.
```

### Missing or Invalid Manifest

```
Error: junebug/repo is missing jolene.toml
  Every jolene package must include a jolene.toml manifest.
  See https://github.com/jolene-pm/jolene#package-format
```

```
Error: Invalid jolene.toml in junebug/repo
  Missing required field: license
```

### No Content Directories

```
Error: junebug/repo has no installable content.
  Expected at least one of: commands/, skills/, agents/
```

### Unknown Target

```
Error: Unknown target 'vscode'.
  Supported targets: claude-code, opencode, codex
```

### No Targets Detected

```
Error: No supported targets detected.
  None found: ~/.claude/, ~/.config/opencode/, ~/.codex/
  Use --to <target> to specify a target explicitly.
```

### Package Conflict

```
Error: Conflict installing junebug/my-tools to claude-code:
  commands/review.md is already provided by other-author/review-pack

  To resolve: jolene uninstall other-author/review-pack --from claude-code
```

### User File Conflict

```
Error: Conflict installing junebug/my-tools to claude-code:
  commands/review.md already exists and is not managed by jolene.
  Remove or rename ~/.claude/commands/review.md, then retry.
```

### Ambiguous Package Name

```
Error: Ambiguous name 'review-tools'. Multiple matches:
  author-a/review-tools
  author-b/review-tools

  Use the full Author/repo format.
```

### Missing Marketplace Manifest

```
Error: No .claude-plugin/marketplace.json found in acme-corp/tools
  Are you sure this is a marketplace repo?
```

### Missing --pick Flag

```
Error: --pick is required with --marketplace
  Use `jolene contents --marketplace --github acme-corp/tools` to see available plugins
```

### Plugin Not Found in Marketplace

```
Error: Plugin 'nonexistent' not found in marketplace.
  Available: review-plugin, deploy-tools, security-scanner
```

### Unsupported Plugin Source

```
Error: Plugin 'npm-thing' uses an unsupported source type (npm/pip are not yet supported by jolene)
```

### Partial Failure Rollback

If symlink creation fails partway through, all symlinks created during
this operation are removed. The state file is not modified. The user sees:

```
Error: Conflict at ~/.claude/commands/deploy.md (see above)
  Rolled back 2 symlinks that were created before the error.
  No changes were made.
```

---

## 8. Future Work

Items explicitly out of scope for MVP, documented for future consideration:

### Per-Project Installation

Install packages scoped to a project rather than globally. Key challenges:
- Symlinks in project directories use absolute paths (not portable for VCS).
- Requires a project-level manifest (`jolene.lock`) and `jolene install`
  on each machine (similar to npm/cargo).
- Precedence rules needed between global and project-scoped packages.
- Significantly different UX model from global installation.

### Version Pinning

Lock packages to specific git tags or commits rather than tracking HEAD.
The manifest's `version` field and git tags provide the foundation.

### Package Registry

A searchable index of jolene packages, enabling `jolene search review`
instead of requiring users to know the exact `Author/repo`.

### Dependency Resolution

Packages that depend on other packages. Requires a solver and lock file.

### Private Repository Support

Authenticated git clones for private repos. For MVP, jolene inherits
whatever git credentials are configured on the system.

### Git Ref Support for Marketplace Plugins

Marketplace plugins can declare a `ref` field (tag or branch). Jolene currently
ignores this and tracks HEAD. Future work: checkout the specified ref after
cloning, and respect it during updates.

### Windows Support

Windows requires junction points or developer-mode symlinks. Deferred to
a future version.
