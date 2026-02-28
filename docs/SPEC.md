# Jolene Specification

A package manager for coding agent commands, skills, and agents.

## Overview

Jolene installs packages from GitHub repositories that bundle commands, skills,
and agents for coding AI tools. Packages are cloned locally and installed to
target tool directories using symlinks.

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

```
jolene install <source> [--to <target>...]
```

- `<source>` — GitHub repository in `Author/repo` format.
- `--to <target>` — Target slug(s). Repeatable. If omitted, installs to all
  targets whose config root directory exists on the system.

**Process:**

1. Clone (or pull if already cloned) the repo into the local store.
2. Validate: repo must have `jolene.toml` and at least one content directory.
3. For each target, create symlinks for all supported content types.
4. Record installation in the state file.

**Example:**

```
$ jolene install junebug/review-tools --to opencode
Installing junebug/review-tools...
  Cloning https://github.com/junebug/review-tools.git
  Found: 1 command, 2 skills

  Installing to opencode:
    + commands/review.md -> ~/.config/opencode/commands/review.md
    + skills/code-analysis/ -> ~/.config/opencode/skills/code-analysis/
    + skills/style-check/ -> ~/.config/opencode/skills/style-check/

Installed junebug/review-tools to opencode
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
    Targets: opencode, claude-code
    Content: 1 command, 2 skills
    Version: 1.0.0 (main@abc1234)

  junebug/agent-learning-system
    Targets: claude-code
    Content: 6 commands
    Version: 0.2.0 (main@def5678)
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

### jolene doctor

```
jolene doctor
```

Verifies health of all installations:
- Checks all recorded symlinks exist and resolve to valid targets.
- Reports broken symlinks, missing clones, and orphaned symlinks.

---

## 2. Package Format

A jolene package is a GitHub repository with a `jolene.toml` manifest and
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

---

## 3. Local Store

All jolene data lives under `~/.jolene/`.

### Directory Layout

```
~/.jolene/
  state.toml                        # installation state
  repos/                            # cloned repositories
    junebug/
      review-tools/                 # git clone
      agent-learning-system/        # git clone
```

### State File: state.toml

Tracks installed packages and their symlinks.

```toml
[[packages]]
source = "junebug/review-tools"
clone_path = "repos/junebug/review-tools"
branch = "main"
commit = "abc1234def5678"
installed_at = "2026-02-28T10:00:00Z"
updated_at = "2026-02-28T10:00:00Z"

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
- `src` paths are relative to the package clone root.
- `dst` paths use `~` for home directory (expanded at runtime).
- `clone_path` is relative to `~/.jolene/`.

**Atomicity:** The state file is written to a temp file then renamed,
preventing corruption on interruption.

---

## 4. Installation Process

### Install: Step by Step

```
1. PARSE source into author + repo. Build GitHub URL.

2. FETCH
   - Exists in store? → git fetch && git pull
   - New? → git clone into ~/.jolene/repos/{author}/{repo}/

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
symlink_source (absolute): /Users/you/.jolene/repos/junebug/review-tools/commands/review.md
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
~/.claude/commands/review.md → /home/user/.jolene/repos/junebug/review-tools/commands/review.md
```

### Directory-Level Symlinks (Skills)

Each skill directory is symlinked whole to preserve internal structure:

```
~/.claude/skills/code-analysis/ → /home/user/.jolene/repos/junebug/review-tools/skills/code-analysis/
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

### Windows Support

Windows requires junction points or developer-mode symlinks. Deferred to
a future version.
