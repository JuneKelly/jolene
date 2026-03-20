# Research: Requirements File for Jolene

**Status:** Research
**Date:** 2026-03-20

---

## Problem

There is currently no way to describe, in a project repository, which Jolene
packages a contributor needs to work on that project. Every developer must
discover and install the relevant packages manually — often by reading a
README, copying a command, or following institutional knowledge. This creates
friction for onboarding and makes it easy for contributors to be missing tools
or to use the wrong ones.

A requirements file would let a project commit its tool expectations to version
control. Running a single `jolene` command from the project directory would
install everything at once, with the right options.

---

## Prior Art

### asdf / `.tool-versions`

The simplest approach. One tool per line, name and version separated by space:

```
golang 1.24.0
nodejs 20.4.0
python 3.10.0
```

**Trigger:** Explicit — `asdf install`. No side effects on directory entry.

**Characteristics:**
- Minimal format; easy to read and write
- No per-tool options beyond version
- Multiple fallback versions can appear on the same line
- Hierarchical: files in parent directories are consulted if the local file is
  missing a tool

**Limitations:** Single-version-per-line format doesn't compose well when each
package needs additional options (prefix, targets, template variables).

---

### mise / `.mise.toml`

A TOML file with richer per-tool configuration:

```toml
[tools]
node = "20.11.0"
python = ["3.11", "3.10"]
terraform = "1.9"

[env]
AWS_REGION = "us-west-2"
```

**Trigger:** Automatic on directory entry (via shell hook), or explicit `mise install`.

**Characteristics:**
- Per-tool install options (OS restrictions, install_env, postinstall scripts)
- Supports arrays of versions for tools that allow multiple active versions
- Separate sections for tools, env vars, settings, and tasks
- Hierarchical: `mise install` merges config up the directory tree

---

### pip / `requirements.txt`

Plain text, one package per line, with optional version constraints and flags:

```
requests==2.28.1
numpy>=1.24.0,<2.0.0
flask[security]>=2.0
-r base-requirements.txt
-e .
pywin32>=1.0 ; sys_platform == 'win32'
```

**Trigger:** Explicit — `pip install -r requirements.txt`.

**Characteristics:**
- Rich version specifiers: `==`, `>=`, `<=`, `!=`, `~=` (compatible release)
- Environment markers for conditional installation: `; sys_platform == 'win32'`
- Extras via brackets: `flask[security]`
- Can reference other requirements files with `-r`
- No built-in environment separation; users manage separate files by convention

---

### Bundler / `Gemfile`

A Ruby DSL — each `gem` call is a function with optional keyword arguments:

```ruby
source 'https://rubygems.org'

gem 'rails', '7.0.0'
gem 'devise', github: 'plataformatec/devise', branch: '4-stable'
gem 'extracted_library', path: './vendor/extracted_library'

group :development, :test do
  gem 'rspec'
  gem 'pry'
end

group :production do
  gem 'pg', '~> 0.18'
end
```

**Trigger:** Explicit — `bundle install`.

**Characteristics:**
- Maximum flexibility via arbitrary per-gem keyword arguments
- Environment groups built into the format
- Git and path sources supported inline
- Lock file (`Gemfile.lock`) records exact resolved versions

**Limitations:** DSL syntax means the file can only be parsed by a Ruby interpreter,
complicating tooling.

---

### Cargo / `Cargo.toml`

TOML with structured per-dependency options:

```toml
[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }
rand = { git = "https://github.com/rust-lang-nursery/rand", branch = "next" }
hello_utils = { path = "./hello_utils" }

[dev-dependencies]
criterion = "0.5"

[target.'cfg(windows)'.dependencies]
winapi = "0.3"
```

**Trigger:** Implicit — `cargo build` fetches and compiles dependencies.

**Characteristics:**
- Rich per-dependency options: version ranges, features, git ref, local path
- Dependency categories (main, dev, build) built into separate sections
- Platform-conditional dependencies via `cfg()` syntax
- Lock file (`Cargo.lock`) for reproducible builds

---

### Summary

| Tool       | Format     | Trigger    | Per-package options     | Version pinning |
|------------|------------|------------|-------------------------|-----------------|
| asdf       | Plain text | Explicit   | None                    | Exact only      |
| mise       | TOML       | Automatic  | OS, env, postinstall    | Exact or range  |
| pip        | Plain text | Explicit   | Extras, env markers     | Ranges          |
| npm        | JSON       | Explicit   | Implicit via categories | Ranges          |
| Cargo      | TOML       | Implicit   | Features, git, path     | Ranges          |
| Bundler    | Ruby DSL   | Explicit   | Extensive               | Ranges          |

Key pattern: tools with rich per-package options (Cargo, Bundler) use structured
data formats (TOML, DSL) rather than line-based formats. Line-based formats
(asdf, pip) either lack per-package options or bolt them on awkwardly as inline
flags.

---

## Jolene-Specific Considerations

### Per-Package Options Are Rich

Unlike most tool managers, each Jolene install can carry significant per-package
configuration:

- **Source type:** `--github`, `--local`, or `--url`
- **Marketplace mode:** `--marketplace` + `--pick <plugins>`
- **Targets:** `--to <target>...` (or auto-detect)
- **Prefix:** `--prefix <value>` or `--no-prefix`
- **Template variable overrides:** `--var key=value` and `--vars-json '{...}'`

A line-based format like `.tool-versions` would require embedding CLI flags in
the line, which is fragile and hard to read:

```
# awkward — not recommended
github:junebug/review-tools --prefix jb --to claude-code --var doc_url=https://internal.corp
```

TOML handles this naturally with inline tables and arrays.

### Naming Convention

Jolene already uses `jolene.toml` for package manifests. A parallel naming
convention for the requirements file would be `jolene-requirements.toml` — clearly
Jolene-specific, distinct from the manifest, and recognisable as a file that
declares intent (what should be installed) rather than what a package provides.

Alternative names considered:

| Name | Notes |
|------|-------|
| `jolene-requirements.toml` | Follows `jolene.toml` convention; clearly not a package manifest |
| `.jolene` | Shorter dotfile; echoes `.tool-versions`; less discoverable |
| `jolene.packages` | Clear but slightly redundant |
| `jolene.install` | Reads as a verb, which is odd for a config file |

`jolene-requirements.toml` is preferred.

### Command: `jolene sync` vs. `jolene install` (no args)

The existing `install` command requires exactly one of `--github`, `--local`,
or `--url`. Changing it to detect a requirements file when called with no
arguments would be a surprising behavioural change.

A new verb, `jolene sync`, is cleaner:

- Reads `jolene-requirements.toml` in the current directory (error if not found)
- Installs each package that is not already installed
- Skips packages that are already installed at the right state
- Does not automatically update packages (that's `jolene update`)

The name `sync` is used by `mise`, `uv`, and other modern tools for
"make the installed state match the declared state". It implies idempotency,
which matches the expected behaviour.

### Idempotency and Existing Installs

When `jolene sync` encounters a package that is already installed:

- **Same source, same options:** Skip. Print "already installed" in verbose mode.
- **Same source, different options (e.g. different prefix):** This is
  ambiguous — the right behaviour is not obvious. Options:
  1. Skip and warn (safest; prevents silent changes)
  2. Error and require the user to uninstall first
  3. Re-install with new options (potentially destructive to other packages
     that depend on the current symlinks)

  Option 1 is recommended as the MVP behaviour.

- **Package not installed:** Install normally.

### Version Pinning

Jolene currently tracks the HEAD/main of each package's default branch.
The `state.json` records the commit hash at install time but `jolene update`
always pulls to the latest. There is no mechanism to pin to a specific git ref.

A requirements file could introduce an optional `ref` field (branch, tag, or
commit). This would be new functionality beyond what the current `install`
command supports.

Two approaches:

1. **Defer version pinning:** MVP requirements file only supports "latest".
   The `ref` field is reserved but not implemented.

2. **Implement ref support:** Add a `ref` field to the requirements file and
   implement `git checkout <ref>` after cloning. This is a standalone feature
   that would benefit the normal `install` command too.

Deferring is simpler for MVP. Version pinning is independent and could be
designed and implemented separately.

### Global Defaults

Some options (e.g. `to`) might apply to all packages in a project. A global
defaults section would reduce repetition:

```toml
[defaults]
to = ["claude-code"]

[[package]]
github = "junebug/review-tools"
# inherits to = ["claude-code"]

[[package]]
github = "acme-corp/tools"
to = ["claude-code", "opencode"]  # override defaults.to
```

This mirrors how Bundler's `source` applies globally unless overridden per gem.

---

## Proposed File Format

Using TOML with an array of tables. Each `[[package]]` entry maps directly to
a `jolene install` invocation.

```toml
# jolene-requirements.toml
# Jolene packages required to work on this project.
# Run `jolene sync` to install.

[[package]]
github = "junebug/review-tools"
prefix = "jb"

[[package]]
github = "junebug/review-tools"
prefix = "jb"
to = ["claude-code", "opencode"]

[package.vars]
doc_url = "https://internal.corp/docs"
show_advanced = true

[[package]]
github = "acme-corp/tools"
marketplace = true
pick = ["review-plugin", "deploy-tools"]

[[package]]
local = "./shared-tools"
prefix = "local"
to = ["claude-code"]

[[package]]
url = "https://gitlab.com/someone/cool-skills.git"
no_prefix = true
```

### Field Reference

Each `[[package]]` entry supports:

| Field          | Type       | Maps to CLI flag         | Notes |
|----------------|------------|--------------------------|-------|
| `github`       | string     | `--github <owner/repo>`  | Mutually exclusive with `local` and `url` |
| `local`        | string     | `--local <path>`         | Relative paths resolved from the file's directory |
| `url`          | string     | `--url <git-url>`        | |
| `marketplace`  | bool       | `--marketplace`          | Requires `pick`; only valid with `github` or `url` |
| `pick`         | string[]   | `--pick <name>,...`      | Required when `marketplace = true` |
| `to`           | string[]   | `--to <target>...`       | If absent, inherits `[defaults].to`; if no default, auto-detect |
| `prefix`       | string     | `--prefix <value>`       | Mutually exclusive with `no_prefix` |
| `no_prefix`    | bool       | `--no-prefix`            | Mutually exclusive with `prefix` |
| `[package.vars]` | table    | `--vars-json '{...}'`    | Merged as a JSON object override |

A top-level `[defaults]` section may set `to` for all packages:

```toml
[defaults]
to = ["claude-code"]
```

---

## Open Questions

**1. What is the precise idempotency behaviour when options diverge?**

If a package is already installed with `prefix = "jb"` and the requirements file
now has `prefix = "jk"`, should `jolene sync` warn and skip, error, or re-install?
The safest MVP answer is: warn and skip (matching the behaviour of `pip install -r`
when a package is already at a compatible version). The user can run
`jolene uninstall` and re-sync if they want to change the prefix.

**2. Should `jolene install` detect `jolene-requirements.toml` as a shorthand?**

Symmetry with how `cargo build` auto-discovers `Cargo.toml` might suggest that
`jolene install` (no args) could also look for `jolene-requirements.toml`. However,
this breaks the existing argument parsing contract (exactly one source flag
required). A separate `sync` command is cleaner.

**3. Should `jolene sync` update already-installed packages, or only install missing ones?**

`npm install` installs missing packages and upgrades existing ones to satisfy
version constraints. `bundle install` does the same. But since Jolene has no
version constraints in the MVP file format (just "latest"), updating on every
`sync` would be surprising — it would make `jolene sync` an alias for
`jolene update`. Recommendation: MVP `sync` installs missing packages only;
`jolene update` remains the upgrade path.

**4. Should there be a `jolene sync --check` dry-run mode?**

A dry run that lists what would be installed/skipped without making changes
would be useful for CI and for reviewing before running. This is analogous to
`bundle check`. Low implementation cost, high value for automation.

**5. How should `local` paths be resolved?**

When `local = "./shared-tools"` appears in `jolene-requirements.toml`, the path
should be resolved relative to the directory containing `jolene-requirements.toml`,
not the current working directory. This is consistent with how `Cargo.toml`
handles path dependencies.

**6. Is `[package.vars]` the right syntax for template variable overrides?**

TOML uses `[package.vars]` for the sub-table of the current `[[package]]`
array entry. An alternative is `vars = { doc_url = "..." }` as an inline table
on the `[[package]]` entry itself. The sub-table form is more readable for
multi-key overrides:

```toml
[[package]]
github = "foo/bar"

[package.vars]
doc_url = "https://internal.corp"
show_advanced = true
max_retries = 5
```

vs.

```toml
[[package]]
github = "foo/bar"
vars = { doc_url = "https://internal.corp", show_advanced = true, max_retries = 5 }
```

The sub-table form scales better for packages with many overrides.

**7. Should `--vars-json` style (JSON strings) also be supported?**

Users already know `--vars-json` from the CLI. The requirements file could
support a `vars_json` field for complex types (arrays, nested objects) that
TOML inline tables handle awkwardly for certain use cases. However, TOML
natively supports arrays and inline tables, making this unnecessary.

**8. Should there be a `jolene add` command to append to the requirements file?**

`npm install <pkg>` appends to `package.json`. A `jolene add --github owner/repo`
that installs and appends to `jolene-requirements.toml` would be ergonomic. This is
optional for MVP — manual editing is sufficient — but worth designing for
compatibility.

---

## Implementation Sketch

The implementation involves three main additions:

### 1. `jolene-requirements.toml` parser (`src/requirements.rs`)

A new module that reads and parses `jolene-requirements.toml` into a `Requirements`
struct. Each `PackageRequirement` closely mirrors the `install` command's
argument structure, so validation can reuse existing logic (prefix validation,
target slug parsing, var type checking).

### 2. `jolene sync` command (`src/commands/sync.rs`)

The new command:

1. Find `jolene-requirements.toml` in the current directory (error if absent)
2. Parse it into a list of `PackageRequirement` entries
3. Load the current state (read-only pass)
4. For each requirement: determine if the package is already installed with
   compatible options (skip with note) or not yet installed (queue for install)
5. Acquire the state lock
6. For each queued requirement: run the full install flow (identical to
   `commands::install::run()`, driven by `PackageRequirement` instead of
   CLI args)
7. Report results

Reusing `install::run()` internally means all existing install logic —
conflict detection, rollback, template rendering, state recording — works
without duplication.

### 3. CLI entry in `cli.rs`

A new `Sync` variant in the `Commands` enum, with no required arguments and
optional `--check` / `--dry-run` flag.

---

## Relationship to Other Features

- **Templating:** Template variable overrides in the requirements file use the
  same `VarValue` type and validation logic as `--var` / `--vars-json`.
- **Prefixes:** Prefix handling is unchanged; the requirements file is just
  another way to supply the same inputs.
- **Version pinning:** A future `ref` field would require new `git` module
  support (checkout after clone). This is independent of the requirements file
  format itself and can be added later.
- **`jolene doctor`:** Could be extended to warn when installed packages are
  not covered by `jolene-requirements.toml` (orphan detection for requirements-managed
  installs).
