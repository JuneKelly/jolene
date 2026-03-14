# Proposal: Templating in Jolene Packages

**Status:** Ready
**Date:** 2026-03-08

---

## Problem

A package may ship interdependent content items — for example, a skill that
instructs users to invoke a companion command. When a user installs the package
with a prefix (e.g. `--prefix xyz`), the command is installed as `xyz--bar`,
but any prose inside the skill that references `bar` by name is not updated.
The reference is silently broken.

Templating at install time resolves this: content files can embed expressions
that are evaluated with full knowledge of the install context (prefix, target,
user-defined variables) before symlinks are created.

---

## Overview

Package authors embed Jolene template expressions in command, skill, and agent
files. Expressions are evaluated at install time using a restricted MiniJinja
environment. Rendered output is written to `~/.jolene/rendered/{hash}/`; the
symlink for that file (or skill directory) points at the rendered copy instead
of the raw clone. Files with no template expressions are unaffected — they are
symlinked to the raw clone as today.

---

## 1. Template Syntax

MiniJinja is used with custom delimiters chosen to be visually distinct and
unlikely to collide with Jinja2, Handlebars, Go templates, or other syntax that
may appear literally inside instructional content.

| Construct  | Syntax                             | Purpose                               |
|------------|------------------------------------|---------------------------------------|
| Expression | `{~ expr ~}`                       | Output a value                        |
| Block      | `{%~ tag ~%}`                      | Control flow (`if`, `for`)            |
| Comment    | `{#~ text ~#}`                     | Template-only comment, not emitted    |
| Escape     | `{%~ raw ~%}...{%~ endraw ~%}`     | Emit literally, no processing         |

Whitespace trimming uses `-` placed immediately inside the delimiter:
`{~- expr -~}` strips surrounding whitespace; `{%~- tag -~%}` does the same
for blocks. In practice this is rarely needed for markdown content.

**Example:**

```text
Run /{~ jolene.resolve("deploy") ~} to deploy.

{%~ if jolene.target == "claude-code" ~%}
Invoke it as a slash command: `/{~ jolene.resolve("deploy") ~}`.
{%~ elif jolene.target == "codex" ~%}
Note: the deploy command is unavailable on Codex — only skills are supported.
{%~ endif ~%}

API docs: {~ jolene.vars.doc_url ~}

{%~ raw ~%}
(This text contains {~ literal Jolene syntax ~} that won't be processed.)
{%~ endraw ~%}
```

---

## 2. Template Context

Everything is namespaced under `jolene`. Nothing else is in scope.
`UndefinedBehavior::Strict` is set — any reference to an undefined name is a
hard error at install time, caught before any symlinks are created.

| Reference                       | Type     | Value                                                                                  |
|---------------------------------|----------|----------------------------------------------------------------------------------------|
| `jolene.resolve("name")`        | Function | Installed name of content item `name`, with prefix applied. Errors if `name` is not declared in the package. If the same name appears in multiple content types, a second argument is required: `jolene.resolve("name", "command")`. |
| `jolene.prefix`                 | String   | The active prefix, or `""` if none.                                                    |
| `jolene.target`                 | String   | Target slug: `"claude-code"`, `"opencode"`, or `"codex"`.                              |
| `jolene.package.name`           | String   | Package name from manifest.                                                             |
| `jolene.package.version`        | String   | Package version from manifest.                                                          |
| `jolene.vars.*`                 | Mixed    | Package-defined variables from `[template.vars]`, overridable via `--var` / `--vars-json`. Scalars (string, bool, int, float), arrays, and nested objects are supported. |

No MiniJinja built-in filters, functions, or globals are registered; only the custom `jolene` global is in scope. `{% if %}`
and `{% for %}` control flow are available (core MiniJinja, not gated by the
`builtins` feature). `{% macro %}`, `{% include %}`, and `{% extends %}` are
disabled via feature flags. A fuel limit of `50_000` operations caps
pathological templates (a conservative default to prevent accidental infinite
loops or malicious packages from locking up machine resources).

`jolene.vars.*` supports scalars, arrays, and nested objects, enabling `for`
iteration over declared vars:

```
{%~ for channel in jolene.vars.notify_channels ~%}
- {~ channel ~}
{%~ endfor ~%}
```

Inline array literals also work without a declared var:

```
{%~ for model in ["claude-opus-4-6", "claude-sonnet-4-6"] ~%}
- {~ model ~}
{%~ endfor ~%}
```

The `{% set %}` tag is part of the `builtins` feature (disabled) and is not
available, but it is not needed for array iteration.

---

## 3. Manifest Additions

One new optional section:

```toml
[package]
name = "review-tools"
# ... existing fields ...

[template.vars]
doc_url          = "https://example.com/docs"
model_hint       = "claude-opus-4-6"
show_advanced    = false
max_retries      = 3
notify_channels  = ["slack", "email"]
db               = {host = "localhost", port = 5432}
```

`[template.vars]` declares variables available as `jolene.vars.*` in templates.
Values may be any TOML native type except datetime: strings, booleans, integers,
floats, arrays, or inline tables (nested objects). The section is optional —
packages without it have no `jolene.vars.*` available.

Booleans enable clean conditional sections without string comparison:

```
{%~ if jolene.vars.show_advanced ~%}
Advanced usage: ...
{%~ endif ~%}
```

Arrays enable iteration over declared lists:

```
{%~ for ch in jolene.vars.notify_channels ~%}
- {~ ch ~}
{%~ endfor ~%}
```

Nested objects enable structured configuration:

```
DB host: {~ jolene.vars.db.host ~}
DB port: {~ jolene.vars.db.port ~}
```

No other manifest changes. Templated files are **detected automatically** by
scanning for the opening delimiters `{~`, `{%~`, or `{#~`; authors do not
need to declare which files use templating.

---

## 4. CLI Additions

Two new repeatable flags on `install`:

```
jolene install --github foo/bar \
  --var doc_url=https://internal.corp/docs \
  --var show_advanced=true \
  --vars-json '{"notify_channels": ["slack", "pagerduty"]}'
```

**`--var key=value`** overrides a single scalar variable. The key is the
substring before the first `=`; the value is everything after it, including
any further `=` characters (e.g. `--var webhook=https://foo.com?a=1&b=2` is
valid). Values are parsed with type inference: `true`/`false` → bool, integer
strings → integer, float strings → float, anything else → string. A type
mismatch against the declared type is an error. Arrays and nested objects
cannot be expressed via `--var`; use `--vars-json` instead.

**`--vars-json '{...}'`** accepts a JSON object and overrides any number of
variables at once. Values may be any JSON type except `null`: strings, booleans,
numbers, arrays, or nested objects. Nested objects are accessible as
`jolene.vars.foo.bar` attribute access in templates. When a value is a nested
object, it is deep-merged into the accumulated value for that key (starting
from the manifest default): keys present in the override are updated, absent
keys are retained. All other value types (scalars, arrays) replace the
accumulated value entirely.

Both flags are repeatable and may be freely mixed. They are processed
left-to-right in the order given, applied on top of the manifest defaults: for
nested object keys, each successive occurrence is deep-merged with the
accumulated value so far; for scalar and array keys, the last value wins.
Referencing a key not declared in `[template.vars]` is an error for both flags
(prevents silent typos). Override values are stored in the state file alongside
the prefix so that `jolene update` re-renders with the same overrides.

---

## 5. Rendered Shadow Store

```
~/.jolene/
  repos/                        # raw git clones (unchanged)
    {hash}/
  rendered/                     # new: rendered copies
    {hash}/
      claude-code/              # per-target (jolene.target varies)
        commands/
          review.md             # rendered
        skills/
          foo/                  # entire dir copied if any file is templated
            SKILL.md            # rendered
            other.md            # copied as-is
      opencode/
        commands/
          review.md             # may differ (target-conditional content)
```

**Key properties:**

- Only files (or skill directories) that contain template expressions get a
  rendered copy. Everything else symlinks to `repos/` as today.
- Skills are rendered at the **directory level**: if any file within a skill
  directory contains a template expression, the entire directory is copied and
  rendered into `rendered/{hash}/{target}/skills/{name}/`. Files within the
  skill directory that contain no expressions are copied as-is. This preserves
  the directory-level symlink model.
- Rendered files are **per-target** to correctly handle `{%~ if jolene.target == "..." ~%}`
  conditionals when a package is installed to multiple targets.
- The `rendered/` directory is an implementation detail — not exposed in state
  file `src` paths (those remain relative to the package root). The symlink
  layer resolves to `rendered/` or `repos/` at runtime based on whether a
  rendered copy exists.
- The store hash is derived from the source identity (e.g.
  `sha256("github||owner/repo")`), not from content. Re-installing the same
  package after a `--purge` reuses the same `rendered/{hash}/` path. Old
  rendered copies do not accumulate on reinstall.

---

## 6. Install Flow Changes

Scan and validate steps are inserted after step 3 (validate); rendering is
inserted after step 5 (resolve targets), since it requires both the prefix and
the target list:

```
3.  VALIDATE (unchanged)

3c. VALIDATE CLI OVERRIDES
    If --var or --vars-json flags were given:
    - Parse --vars-json values as JSON; error if the top-level value is not
      a JSON object, or if any value within it is null.
    - For each key across all override flags, verify it is declared in [template.vars].
    - For each --var value, verify the inferred type matches the declared type.
    This step is fatal and runs regardless of whether any content is templated.

3d. SCAN FOR TEMPLATES
    For each declared content item:
    - Commands/Agents: read the .md file, scan for {~, {%~, or {#~
    - Skills: scan every file recursively in the skill directory
    Mark each ContentItem as templated: bool.

3e. VALIDATE TEMPLATE EXPRESSIONS
    For each templated item, parse the template AST and extract all
    jolene.resolve() calls and jolene.vars.* references:
    - Verify each resolve() first argument names a declared content item.
    - If a name appears in multiple content types and no second argument is
      provided, error as ambiguous.
    - Verify each resolve() second argument (if present) is a valid content
      type string: "command", "skill", or "agent".
    - Verify each vars.* key is declared in [template.vars].
    - Error immediately with a descriptive message on any invalid reference.
    This step is fatal — invalid references abort the install.

4.  RESOLVE PREFIX (unchanged)
5.  RESOLVE TARGETS (unchanged)

5b. RENDER TEMPLATES
    For each target × each templated content item:
    - Render using MiniJinja with the full jolene context for that target.
    - Write rendered output to ~/.jolene/rendered/{hash}/{target}/{relative_path}.
    - For skills: copy the entire directory, rendering templated files,
      copying non-templated files as-is.
    Non-templated items: no action; their symlinks will point to repos/.

6.  CHECK CONFLICTS — symlink src is rendered/{hash}/{target}/... or repos/{hash}/...
7.  CREATE DIRECTORIES (unchanged)
8.  CREATE SYMLINKS — src is rendered path if templated, clone path if not
9.  RECORD STATE — include var_overrides if any --var or --vars-json flags were set
```

---

## 7. Update Flow Changes

After `git pull`, re-render before re-symlinking:

```
1.  git pull (unchanged)
2.  Detect content changes (unchanged)

2b. RE-SCAN AND RE-RENDER
    - Re-scan all content items for template expressions.
    - Before re-rendering, validate var_overrides against the updated
      [template.vars]. If any stored override key is no longer declared,
      or its declared type changed and the stored value no longer matches,
      abort with an error directing the user to uninstall and reinstall with corrected
      overrides: `jolene uninstall <pkg> && jolene install [--var ...] [--vars-json ...]`.
    - Re-render all templated items using var_overrides stored in state.
    - Write new rendered copies (overwriting existing in rendered/{hash}/).
    - Items previously templated (templated: true in their SymlinkEntry) but
      no longer containing expressions: delete their rendered copy; remove and
      recreate the symlink pointing to repos/; set templated: false in state.
    - Items newly containing expressions: create rendered copy; remove and
      recreate the symlink pointing to rendered/; set templated: true in state.

3.  Create symlinks for new content (unchanged)
4.  Remove symlinks for deleted content (unchanged)
5.  Update state (unchanged)
```

---

## 8. Uninstall / Purge Changes

`--purge` already deletes `repos/{hash}/`. It now also deletes
`rendered/{hash}/` if present.

Without `--purge`, rendered files are left in place (same as clones). `jolene
doctor` reports orphaned `rendered/` directories: it enumerates every
subdirectory of `~/.jolene/rendered/` and cross-references each hash against
the set of `store_key` values in state. Any hash present on disk but absent
from state is reported as orphaned.

---

## 9. Error Handling

**Invalid `resolve()` target:**

```
Error: Template error in skills/foo/SKILL.md:
  jolene.resolve("baz") references content item 'baz', which is not
  declared in this package.
  Declared items: bar (command), foo (skill)
```

**Unknown `vars` key:**

```
Error: Template error in commands/review.md:
  jolene.vars.typo_url is not declared in [template.vars].
  Declared vars: doc_url, model_hint
```

**Ambiguous `resolve()` (name in multiple content types):**

```
Error: Template error in skills/foo/SKILL.md:
  jolene.resolve("review") is ambiguous — 'review' exists as both a
  command and a skill.
  Use jolene.resolve("review", "command") to disambiguate.
```

**`resolve()` invalid content type:**

```
Error: Template error in skills/foo/SKILL.md:
  jolene.resolve("review", "banana"): "banana" is not a valid content type.
  Valid types: command, skill, agent.
```

**`--var` type mismatch:**

```
Error: --var show_advanced=hello: declared as bool in [template.vars], expected true or false.
```

**`--vars-json` parse failure:**

```
Error: --vars-json: invalid JSON: expected value at line 1 column 2.
```

**`--vars-json` top-level not an object:**

```
Error: --vars-json: expected a JSON object at the top level, got string.
```

**`--vars-json` unknown key:**

```
Error: --vars-json: key 'typo_url' is not declared in [template.vars].
  Declared vars: doc_url, model_hint, notify_channels
```

**`--vars-json` type mismatch:**

```
Error: --vars-json: key 'notify_channels' declared as array in [template.vars],
  but got a string value.
```

**`--vars-json` disallowed value type:**

```
Error: --vars-json: key 'status' has a null value, which is not supported.
  Permitted types: string, bool, number, array, object.
```

**Stale variable override on update (removed key):**

```
Error: Stored variable override 'old_key' is no longer declared in [template.vars].
  The package update removed this variable. Uninstall and reinstall with corrected overrides:
    jolene uninstall owner/repo && jolene install --github owner/repo [--var key=value] [--vars-json ...]
  Declared vars: doc_url, model_hint
```

**Stale variable override on update (type changed):**

```
Error: Stored variable override 'show_advanced' has type bool, but [template.vars]
  now declares it as string. Uninstall and reinstall with corrected overrides:
    jolene uninstall owner/repo && jolene install --github owner/repo [--var key=value] [--vars-json ...]
  Declared vars: doc_url, model_hint, show_advanced
```

In both cases the source flag (`--github owner/repo`, `--local /path/to/dir`,
or `--url https://...`) is substituted from the value stored in state — it is
not hardcoded to `--github`.

**MiniJinja syntax error:**

```
Error: Template syntax error in commands/review.md (line 14):
  unexpected end of variable block, expected ~}
```

**Fuel exhausted:**

```
Error: Template in skills/foo/SKILL.md exceeded execution limit.
  Possible infinite loop in template logic.
```

---

## 10. Cargo Changes

```toml
[dependencies]
minijinja = { version = "2", default-features = false, features = [
    "custom_syntax",   # {~ ~} / {%~ ~%} delimiters
    "fuel",            # execution limit against pathological templates
] }
# builtins, macros, multi_template are all disabled
```

---

## 11. Code Changes

| File | Change |
|---|---|
| **New** `src/template.rs` | MiniJinja env setup; `scan_for_expressions()`; `validate_references()`; `render_content()`; `build_context()` |
| **New** `src/types/var_value.rs` | `VarValue` — JSON-compatible recursive type (string \| bool \| int \| float \| array \| object; no null — equivalent to `serde_json::Value` minus `Null`); re-exported from `src/types/mod.rs`; used by both `manifest.rs` and `state.rs` |
| `src/types/content.rs` | Add `templated: bool` to `ContentItem`; add `rendered_path(rendered_item_root: &Path) -> PathBuf` — mirrors `source_path(clone_root)`; caller passes `config::rendered_path_for(hash, target)` |
| `src/types/manifest.rs` | Parse `[template.vars]` → `HashMap<String, VarValue>` (imported from `types::var_value`); `VarValue` supports all TOML types except datetime: string, bool, integer, float, array, and nested object |
| `src/types/state.rs` | Add `templated: bool` to `SymlinkEntry` with `#[serde(default)]` (existing entries without the field deserialise as `false`; tracks per-item render state for clean updates); add `var_overrides: Option<HashMap<String, VarValue>>` (imported from `types::var_value`) to `PackageState`; serialises naturally to JSON |
| `src/cli.rs` | Add `--var key=value` and `--vars-json '{...}'` (both repeatable, merged left-to-right) to `install` subcommand |
| `src/symlink.rs` | `plan_symlinks()` gains a `rendered_item_root: Option<&Path>` parameter (the pre-computed `rendered_path_for(hash, target)` for this install, or `None` if no items are templated); selects `rendered/` or `repos/` source path per item based on `ContentItem.templated` |
| `src/commands/install.rs` | Add steps 3c–3e (between validation and conflict check) and step 5b (between target resolution and conflict check) |
| `src/commands/update.rs` | Add re-scan and re-render step after pull |
| `src/commands/uninstall.rs` | `--purge` also removes `rendered/{hash}/` |
| `src/commands/doctor.rs` | Report orphaned `rendered/` directories |
| `src/config.rs` | Add `rendered_root()` and `rendered_path_for(hash, target)` helpers |

---

## What Stays Unchanged

- Conflict detection (reads symlink targets, which still point into `~/.jolene/`)
- Marketplace mode — templating is **not applied** to marketplace-sourced content. Marketplace plugins are not expected to contain Jolene-specific syntax; any `{~` or `{%~` sequences in marketplace content are left as-is.
- `--prefix` / `--no-prefix` (prefix is resolved before rendering; `jolene.prefix` and `jolene.resolve()` are always correct)
- `jolene list`, `jolene info`, `jolene contents`, `jolene doctor` (no changes except orphan detection in doctor)
