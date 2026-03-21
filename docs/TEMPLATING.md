# Templating Guide

Jolene supports template expressions in bundle content files. Templates are
evaluated at install time with full knowledge of the install context — prefix,
target, user-defined variables — so that cross-references between content items
and target-conditional prose are always correct.

---

## Why Templating

### The prefix problem

A bundle ships a skill and a companion command. The skill's instructions tell
users to invoke `/deploy`. When installed with `--prefix acme`, the command
becomes `acme--deploy`, but the skill still says `/deploy`. The reference is
silently broken.

With templating:

```text
Run /{~ jolene.resolve("deploy") ~} to deploy.
```

This renders to `acme--deploy` when installed with `--prefix acme`, and
`deploy` when installed without a prefix.

### Target-conditional content

A bundle installed to multiple targets may need different instructions per
target. Without templating, authors must either write generic prose or maintain
separate files. With templating:

```text
{%~ if jolene.target == "claude-code" ~%}
Invoke as a slash command: `/{~ jolene.resolve("deploy") ~}`.
{%~ elif jolene.target == "codex" ~%}
Note: commands are not supported on Codex — only skills are available.
{%~ endif ~%}
```

### User-customisable content

Bundles can declare variables with sensible defaults that users override at
install time:

```text
API documentation: {~ jolene.vars.doc_url ~}
```

A team installs the same bundle with `--var doc_url=https://internal.corp/api`
to point at their internal docs.

---

## Syntax Reference

Jolene uses [MiniJinja](https://github.com/mitsuhiko/minijinja) with custom
delimiters chosen to avoid collisions with Jinja2, Handlebars, Go templates,
and other syntax that commonly appears in instructional markdown.

### Delimiters

| Construct  | Syntax                             | Purpose                            |
|------------|------------------------------------|------------------------------------|
| Expression | `{~ expr ~}`                       | Output a value                     |
| Block      | `{%~ tag ~%}`                      | Control flow (`if`, `for`)         |
| Comment    | `{#~ text ~#}`                     | Template-only comment, not emitted |
| Escape     | `{%~ raw ~%}...{%~ endraw ~%}`     | Emit content literally             |

### Whitespace trimming

Place `-` immediately inside the delimiter to strip adjacent whitespace:

```text
{~- expr -~}       strips whitespace on both sides
{~- expr ~}        strips whitespace before only
{~ expr -~}        strips whitespace after only
{%~- if cond -~%}  same for blocks
```

In practice this is rarely needed for markdown content.

### Raw blocks

To include literal Jolene delimiters without processing:

```text
{%~ raw ~%}
This {~ syntax ~} is emitted literally and not processed.
{%~ endraw ~%}
```

### Control flow

Standard `if`/`elif`/`else`/`endif` and `for`/`endfor` are available:

```text
{%~ if jolene.vars.show_advanced ~%}
Advanced usage: ...
{%~ endif ~%}

{%~ for channel in jolene.vars.notify_channels ~%}
- {~ channel ~}
{%~ endfor ~%}
```

Inline array literals work without a declared variable:

```text
{%~ for model in ["claude-opus-4-6", "claude-sonnet-4-6"] ~%}
- {~ model ~}
{%~ endfor ~%}
```

### Other available tags

- **`{%~ set x = expr ~%}`** — assign a local variable within the template.

### What is not available

The following MiniJinja features are disabled:

- **Macros** (`{% macro %}`) — disabled via feature flags
- **Includes** (`{% include %}`) — disabled via feature flags
- **Extends** (`{% extends %}`) — disabled via feature flags
- **Built-in filters and functions** — only the `jolene` global is in scope.
  Standard MiniJinja filters like `upper`, `lower`, `join` are not available.

---

## Template Context

Everything is namespaced under `jolene`. Nothing else is in scope. Referencing
an undefined name is a hard error at install time (before any symlinks are
created).

### `jolene.resolve(name)` / `jolene.resolve(name, type)`

Returns the installed name of a content item, with the active prefix applied.

```text
Run /{~ jolene.resolve("deploy") ~} to deploy.
```

If `deploy` is a command and the bundle is installed with `--prefix acme`,
this renders to `acme--deploy`.

When a name appears in multiple content types (e.g. both a command and a skill
named `review`), a second argument is required to disambiguate:

```text
The /{~ jolene.resolve("review", "command") ~} command uses the
{~ jolene.resolve("review", "skill") ~} skill internally.
```

Valid type strings: `"command"`, `"skill"`, `"agent"`.

Errors:

- Name not declared in the bundle → error listing declared items
- Ambiguous name without disambiguator → error suggesting the second argument
- Invalid type string → error listing valid types

### `jolene.prefix`

The active prefix as a string, or `""` if no prefix is set.

```text
{%~ if jolene.prefix ~%}
All commands in this bundle are prefixed with "{~ jolene.prefix ~}".
{%~ endif ~%}
```

### `jolene.target`

The target slug: `"claude-code"`, `"opencode"`, or `"codex"`.

```text
{%~ if jolene.target == "claude-code" ~%}
Use the slash command: `/{~ jolene.resolve("deploy") ~}`.
{%~ elif jolene.target == "codex" ~%}
Note: commands are not supported on Codex.
{%~ endif ~%}
```

Since templates are rendered per-target, a bundle installed to both
`claude-code` and `opencode` produces different rendered output for each.

### `jolene.bundle.name` / `jolene.bundle.version`

The bundle name and version from `jolene.toml`:

```text
Provided by {~ jolene.bundle.name ~} v{~ jolene.bundle.version ~}.
```

### `jolene.vars.*`

Variables declared in `[template.vars]` in the manifest. Supports strings,
booleans, integers, floats, arrays, and nested objects.

```text
API docs: {~ jolene.vars.doc_url ~}
Max retries: {~ jolene.vars.max_retries ~}
DB host: {~ jolene.vars.db.host ~}
```

---

## Declaring Variables

Add a `[template.vars]` section to `jolene.toml`:

```toml
[template.vars]
doc_url          = "https://example.com/docs"
model_hint       = "claude-opus-4-6"
show_advanced    = false
max_retries      = 3
notify_channels  = ["slack", "email"]
db               = { host = "localhost", port = 5432 }
```

Values may be any TOML type except datetime:

| TOML type    | Example                          | Template access              |
|--------------|----------------------------------|------------------------------|
| String       | `"hello"`                        | `{~ jolene.vars.key ~}`      |
| Boolean      | `true` / `false`                 | `{%~ if jolene.vars.key ~%}` |
| Integer      | `42`                             | `{~ jolene.vars.key ~}`      |
| Float        | `3.14`                           | `{~ jolene.vars.key ~}`      |
| Array        | `["a", "b"]`                     | `{%~ for x in jolene.vars.key ~%}` |
| Inline table | `{ host = "localhost", port = 5432 }` | `{~ jolene.vars.key.host ~}` |

The `[template.vars]` section is optional. Bundles that only use
`jolene.resolve()`, `jolene.prefix`, `jolene.target`, or `jolene.bundle.*`
do not need it.

---

## User Overrides

Users can override declared variables at install time with `--var` and
`--vars-json`. Overrides are stored in the state file and preserved across
`jolene update`.

### `--var key=value`

Override a single scalar variable. Repeatable.

```sh
jolene install --github foo/bar \
  --var doc_url=https://internal.corp/docs \
  --var show_advanced=true \
  --var max_retries=5
```

The key is the substring before the first `=`; the value is everything after
it (so `--var webhook=https://foo.com?a=1&b=2` works correctly).

Values are parsed with type inference:

| Input          | Inferred type |
|----------------|---------------|
| `true`/`false` | Boolean       |
| `42`, `-1`     | Integer       |
| `3.14`, `1e10` | Float         |
| Anything else  | String        |

The inferred type must match the declared type in `[template.vars]`. A
mismatch is an error.

Arrays and nested objects cannot be expressed via `--var`; use `--vars-json`.

### `--vars-json '{...}'`

Override any number of variables at once via a JSON object. Repeatable.

```sh
jolene install --github foo/bar \
  --vars-json '{"notify_channels": ["slack", "pagerduty"]}' \
  --vars-json '{"db": {"host": "db.internal.corp"}}'
```

Values may be any JSON type except `null`. When a value is a nested object, it
is **deep-merged** with the accumulated value for that key: keys present in the
override are updated, absent keys are retained. All other types (scalars,
arrays) replace the accumulated value entirely.

### Processing order

Both flags are repeatable. All `--var` flags are processed first (in order),
then all `--vars-json` flags (in order), applied on top of the manifest
defaults. Within each flag type, the last value for a given key wins.

```sh
jolene install --github foo/bar \
  --var max_retries=5 \
  --var max_retries=10
# Result: max_retries = 10 (last --var wins)
```

Since `--vars-json` is always processed after `--var`, a `--vars-json` override
for the same key will take precedence regardless of flag order on the command
line.

### Validation

- Referencing a key not declared in `[template.vars]` is an error (prevents
  silent typos).
- A type mismatch between the override value and the declared type is an error.
- `null` values in `--vars-json` are not permitted.
- The top-level `--vars-json` value must be a JSON object.

---

## Common Patterns

### Prefix-aware cross-references

The most common use case. A skill references a companion command:

```toml
# jolene.toml
[content]
commands = ["deploy", "rollback"]
skills = ["deploy-guide"]
```

```text
# skills/deploy-guide/SKILL.md
To deploy, run /{~ jolene.resolve("deploy") ~}.
If something goes wrong, run /{~ jolene.resolve("rollback") ~}.
```

### Target-conditional sections

Different instructions per target:

```text
{%~ if jolene.target == "claude-code" ~%}
## Claude Code

Use slash commands: `/{~ jolene.resolve("review") ~}`
{%~ elif jolene.target == "opencode" ~%}
## OpenCode

Use the command palette to run {~ jolene.resolve("review") ~}.
{%~ else ~%}
## Other targets

Run the {~ jolene.resolve("review") ~} command.
{%~ endif ~%}
```

### Iterating over arrays

```toml
# jolene.toml
[template.vars]
supported_languages = ["python", "rust", "typescript"]
```

```text
Supported languages:
{%~ for lang in jolene.vars.supported_languages ~%}
- {~ lang ~}
{%~ endfor ~%}
```

### Nested configuration

```toml
# jolene.toml
[template.vars]
api = { base_url = "https://api.example.com", version = "v2" }
```

```text
Endpoint: {~ jolene.vars.api.base_url ~}/{~ jolene.vars.api.version ~}/resources
```

Users can deep-merge overrides for specific keys:

```sh
jolene install --github foo/bar \
  --vars-json '{"api": {"base_url": "https://api.internal.corp"}}'
# Result: base_url overridden, version retained as "v2"
```

### Conditional features

```toml
# jolene.toml
[template.vars]
enable_experimental = false
```

```text
{%~ if jolene.vars.enable_experimental ~%}
## Experimental Features

These features are not yet stable. Use at your own risk.
...
{%~ endif ~%}
```

### Bundle metadata in content

```text
---
name: review
description: Code review command from {~ jolene.bundle.name ~} v{~ jolene.bundle.version ~}
---

...
```

---

## How It Works

### Detection

Jolene scans each content file for the opening delimiters `{~`, `{%~`, or
`{#~`. Files that contain at least one delimiter are marked as templated.
Authors do not need to declare which files use templating.

> **Note:** Since detection uses simple string matching, files containing these
> delimiter sequences as literal text (e.g., in documentation explaining Jolene
> syntax) will be incorrectly treated as templated. The custom delimiters with
> tildes make this unlikely in practice. Use `[template] exclude` to opt out
> when this occurs.

- **Commands and agents:** The single `.md` file is scanned.
- **Skills:** Every file in the skill directory is scanned recursively. If
  *any* file contains a template expression, the *entire* skill directory is
  treated as templated.

### Opting out of detection

If a content item intentionally contains literal `{~`, `{%~`, or `{#~` (e.g.,
a guide explaining Jolene syntax), add its name to `[template] exclude` in
`jolene.toml`:

```toml
[template]
exclude = ["syntax-guide"]  # never scanned; symlinked to repos/ as-is
```

Excluded items are never scanned or rendered. Their symlinks always point to
`repos/` regardless of file content. The name must be declared in `[content]`;
an unknown name is an error.

### Rendered shadow store

Rendered output is written to `~/.jolene/rendered/{hash}/{target}/`. The
symlink for a templated item points to this rendered copy instead of the raw
clone in `repos/`.

```
~/.jolene/
  repos/{hash}/                    # raw git clone
    commands/review.md             # source with {~ expressions ~}
  rendered/{hash}/
    claude-code/
      commands/review.md           # rendered for claude-code
    opencode/
      commands/review.md           # rendered for opencode (may differ)
```

Non-templated items are symlinked to `repos/` as before. Both paths are under
`~/.jolene/`, so conflict detection works for both.

### Per-target rendering

Templates are rendered once per target because `jolene.target` differs.
A bundle installed to both `claude-code` and `opencode` may produce different
output for each, enabling target-conditional content.

### Skills with mixed files

When a skill directory is marked as templated, Jolene copies the entire
directory into `rendered/`. Files that contain template expressions are
rendered; files that do not (including binary files) are copied as-is. This
preserves the directory-level symlink model.

### What happens during `jolene update`

1. Jolene pulls the latest from the remote.
2. All content items are re-scanned for template expressions.
3. Stored variable overrides are validated against the updated manifest. If a
   stored override key was removed or its type changed, the update aborts with
   a message directing the user to re-install.
4. All templated items are re-rendered using the stored overrides.
5. Items whose templated status changed (gained or lost expressions) have their
   symlinks recreated pointing to the correct source (`rendered/` or `repos/`).

---

## Limitations

- **No built-in filters or functions.** Only the `jolene` global is in scope.
  Standard MiniJinja filters like `upper`, `lower`, `join` are not available.
- **No macros, includes, or extends.** Templates cannot reference other
  templates. Each file is self-contained.
- **Fuel limit.** Template execution is capped at 50,000 operations to prevent
  infinite loops or pathological templates from locking up the machine. This
  limit is generous for any reasonable content file.
- **Marketplace not supported.** Templating applies to native bundles only.
  Marketplace-sourced content is not scanned or rendered. Any `{~` or `{%~`
  sequences in marketplace content are left as-is.
- **No interactive prompts.** Variable overrides must be provided via CLI flags.
  There is no interactive mode for filling in missing variables.
- **TOML datetime not supported.** The `[template.vars]` section supports
  strings, booleans, integers, floats, arrays, and inline tables. TOML
  datetime values are rejected.
