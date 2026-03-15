# Issues

## 001 - `find_package` uses display string as identity key

`state::find_package` (and `find_package_mut`) look up packages by matching
`pkg.source` — the human-readable display string (`owner/repo`, absolute path,
or URL). This works in practice but has a theoretical edge case: if two packages
ever produce the same display string (most plausible with URL sources), lookup
becomes ambiguous or incorrect.

The canonical identity for a package is its store key (64-char SHA256 hex in
`clone_path`). A more robust approach would make store key the primary lookup
path, with the display string as a convenience alias.

Not urgent while the tool is in development and the source types are distinct
enough in practice.

---

## 002 - Inconsistent `--pick` syntax in marketplace install command

**Location:** SPEC.md lines 101 vs 106 (now resolved)

**Problem:** The CLI syntax for `--pick` was contradictory between two sections:

Line 101 showed:
```
jolene install --marketplace --github <org/repo> --pick <plugin>[,<plugin>...]
```
This indicates comma-separated values in a single argument.

But line 106 said:
> `--pick <name>` — Select plugins from the marketplace catalog. **Repeatable.** Comma-separated. Required when `--marketplace` is set.

The word "Repeatable" implied you can use `--pick foo --pick bar`, but the syntax showed comma-separated format. These contradicted each other.

**Resolution (2026-03-14):** Both formats now work:
- CLI implementation (`src/cli.rs`) uses `value_delimiter = ','` which parses both `foo,bar` and `--pick foo --pick bar`
- SPEC.md updated to clarify both formats are supported

**Status:** RESOLVED

---

## 003 - Template detection may produce false positives

**Location:** SPEC.md line 1036-1038

**Problem:** The spec says templated files are detected by scanning for the opening delimiters `{~`, `{%~`, or `{#~`:

> Templated files are detected automatically by scanning for the opening delimiters `{~`, `{%~`, or `{#~`. Authors do not need to declare which files use templating.

This approach can incorrectly flag files as templated when they contain these character sequences literally in their content. For example, a command explanation that includes MiniJinja syntax as documentation would be treated as a template.

**Current mitigation:** The custom delimiters (`{~`, `{%~`, `{#~`) with tildes are unusual enough that false positives are unlikely in practice. However, if someone writes literal template-like text in a non-template context, it could cause:

- Unnecessary rendering overhead
- Template syntax errors if the content isn't actually a valid template

**Recommendation:** The current approach is probably acceptable given how niche the delimiter choice is. Document this behavior clearly so package authors understand what's happening.

**Status:** RESOLVED (2026-03-14) — Added documentation notes to SPEC.md and TEMPLATING.md explaining the false positive possibility.

---

## 004 - Marketplace plugin name collision has no resolution mechanism

**Location:** SPEC.md lines 815-818

**Problem:** The spec allows short-name lookups for marketplace plugins:

> **Marketplace plugins:** `"review-plugin"` matches any package with `plugin_name = "review-plugin"`.

But if two different marketplace repos both have a plugin named "review-plugin", there's no disambiguation mechanism:

- `jolene update review-plugin` becomes ambiguous with no way to specify which marketplace it comes from
- The error message would list both, but there's no syntax like `--from-marketplace` to resolve it

**Current workaround:** Users must use the full qualified name: `jolene update acme-corp/tools::review-plugin`

**Recommendation:** Either:

1. Document that users must use the fully-qualified name when collisions occur
2. Add a `--from-marketplace` flag to disambiguate
3. Require marketplace plugins to always be referenced as `marketplace::plugin` in update commands

**Resolution (2026-03-15):** The fully-qualified composite source key (`acme-corp/tools::review-plugin`) already resolves the collision — `find_package` exact-matches on `p.source` when the argument contains `/`. The error message and SPEC.md have been updated to explain both resolution formats (`owner/repo` for native, `org/marketplace::plugin-name` for marketplace).

**Status:** RESOLVED

---

## 005 - `jolene.resolve()` doesn't validate target support

**Location:** SPEC.md line 1022

**Problem:** The `jolene.resolve("name")` function resolves to the installed name of a content item, but doesn't validate whether that content type is actually supported by the target being installed to.

Consider this scenario:

- Package has both commands and skills
- User installs to Codex (which doesn't support commands)
- A skill template calls `jolene.resolve("some-command")`
- The resolve succeeds, but commands aren't installed to Codex

This could produce confusing behavior where templates resolve to names that don't actually exist in the target's installation.

**Resolution (2026-03-15):** The root issue is that partial installs were silent. Patching `resolve()` would treat a symptom. Instead, the skip warnings in `plan_all_targets()` (`install.rs`) were promoted from `verbose`-only to always-visible, with a `Warning:` prefix. Users now see immediately when a package is only partially supported by the chosen target, and can decide whether to proceed. SPEC.md updated accordingly.

**Status:** RESOLVED

---

## 006 - Prefix not re-validated on update

**Location:** SPEC.md line 976-977

**Problem:** The spec says:

> The prefix is locked at install time and stored in `state.json`. `jolene update` preserves the stored prefix.

However, it doesn't specify what happens if:

1. The manifest's default prefix changes between versions
2. The manifest's prefix is removed entirely
3. The prefix validation rules change (e.g., character restrictions)

Currently, the stored prefix is used directly without checking against current manifest rules.

**Recommendation:** On update, re-validate the stored prefix:

- If prefix no longer passes validation, error and suggest reinstall
- If manifest has a new default and user had no explicit prefix choice, could offer to update (but this changes behavior silently)

Document this behavior explicitly.

**Resolution (2026-03-15):**

- The stored prefix is re-validated against current rules in `update_one` (`src/commands/update.rs`). A prefix valid at install time will always pass re-validation under the stable rules, so this only catches manual state file corruption.
- Manifest prefix changes (author adds, removes, or changes `[package].prefix`) have no effect on existing installs — the stored prefix is always authoritative. This is by design and is now documented explicitly in SPEC.md.
- To change the prefix, the user must uninstall and reinstall.

**Status:** RESOLVED

---

## 007 - Unclear "pre-existing entries" for `source_kind` default

**Location:** SPEC.md line 564

**Problem:** The spec says:

> `source_kind`: `"github"` \| `"local"` \| `"url"`. **Defaults to `"github"` for pre-existing entries.**

What defines a "pre-existing entry"? This term isn't defined anywhere. It likely means:

- State file entries that existed before `source_kind` was added as a field
- Or entries imported/migrated from older state formats

But the spec doesn't explain:

- How to identify a pre-existing entry
- What happens if someone tries to install a local package that matches an old "pre-existing" entry
- Migration path for old state files

**Recommendation:** Clarify:

1. Define what "pre-existing" means in this context
2. Document the migration strategy (if any) for old state files
3. Consider whether `source_kind` should be required for all new installations rather than having a default

**Resolution (2026-03-15):**

"Pre-existing entries" means `PackageState` records in state files that predate the addition of the `source_kind` field (state files from the legacy `state.toml` era, before the opaque SHA256 store was introduced). The `#[serde(default)]` attribute on `source_kind` handles deserialization of such entries by falling back to `SourceKind::GitHub`, which is correct because the legacy format only recorded GitHub packages. The explicit `state.toml` → `state.json` migration in `state.rs` also relies on this default. All packages installed with current jolene have `source_kind` set explicitly. SPEC.md updated to define the term and explain the rationale.

**Status:** RESOLVED

---

## 008 - Marketplace install references wrong step numbers

**Location:** SPEC.md line 800-801

**Problem:** Step 4f in the marketplace install process says:

> 4f. RESOLVE TARGETS, CHECK CONFLICTS, CREATE SYMLINKS
> Same as native install (steps 4-7 above).

But in native install, the steps are:

- Step 4: RESOLVE PREFIX
- Step 5: RESOLVE TARGETS  
- Step 6: CHECK CONFLICTS
- Step 7: CREATE DIRECTORIES
- Step 8: CREATE SYMLINKS

So marketplace step 4f is referencing steps 4-7, but:

1. PREFIX resolution (step 4) is different for marketplace — there's no manifest to read default prefix from
2. The step numbers are off by one (should probably be 4-8)

**Resolution needed:** Update the marketplace install process to reference the correct steps and explain any differences in prefix/content resolution for marketplace plugins.

---

## 009 - Improve template detection to decrease false positives

**Location:** `src/template.rs:38-40`, `docs/SPEC.md`, `docs/TEMPLATING.md`

**Background:**

Template detection currently uses a simple string contains check:

```rust
// src/template.rs:38-40
pub fn scan_for_expressions(content: &str) -> bool {
    content.contains("{~") || content.contains("{%~") || content.contains("{#~")
}
```

This approach has been documented (per issue #003), but a more robust solution would eliminate false positives entirely.

**Current behavior:**
- Files containing `{~`, `{%~`, or `{#~` are marked as templated
- Any file with these sequences is rendered through MiniJinja
- If the content isn't a valid template, MiniJinja throws a syntax error

**Potential approaches:**

1. **Explicit opt-in via manifest** (Recommended)
   - Add `templated = true` field to content item definitions in `jolene.toml`
   - Detection becomes: if manifest says templated OR delimiters found
   - Authors explicitly declare which files use templating
   - Eliminates false positives; shifts burden to authors (but they already declare content)
   - Breaking change for existing packages that rely on auto-detection

2. **Validate-before-render**
   - Try to parse as template first; if parsing fails, treat as plain text
   - MiniJinja's `Environment::template_from_str()` can check syntax without executing
   - More robust but still has edge cases (valid template that happens to fail at runtime)

3. **Require closing delimiters**
   - Only match if both opening AND closing delimiters are present
   - E.g., `{~ ... ~}` requires closing `~}`
   - Reduces false positives but still not perfect (could have valid closing without being template)

4. **Whitespace-sensitive matching**
   - Require specific whitespace around delimiters: `{~ expr ~}` not `foo{~bar`
   - Templates typically have space after opening: `{~`, `{%~`, `{#~`
   - Literal text rarely has exact pattern with tilde

5. **Hybrid approach**
   - Keep current detection as default
   - Allow explicit `templated = false` in manifest to opt-out
   - Packages with false positives can opt out; auto-detection works for most

6. **File extension heuristic**
   - `.md.j2`, `.md.tmpl`, or similar extensions explicitly indicate templating
   - Keep auto-detection as fallback
   - Requires author to rename files

**Trade-offs:**

| Approach | False Positives | Complexity | Breaking Change | Author Burden |
|----------|-----------------|------------|-----------------|---------------|
| Opt-in manifest | Eliminated | Low | Yes | Must declare |
| Validate-first | Reduced | Medium | No | None |
| Closing delimiter | Reduced | Low | No | None |
| Whitespace sensitive | Reduced | Low | No | None |
| Hybrid opt-out | Reduced | Low | No | Opt-out when needed |
| File extension | Reduced | Low | No | Rename files |

**Recommendation:**

Approach #1 (explicit opt-in) or #5 (hybrid opt-out) would completely eliminate false positives. Given that authors already declare content items in `jolene.toml`, adding a `templated` field is consistent with the manifest-based approach. A hybrid where:
- Default remains auto-detection (backward compatible)
- Allow `templated = false` to opt-out of rendering

This addresses false positives without breaking existing packages.

**Related:**
- Issue #003 (original false positive documentation)
- `docs/TEMPLATING.md` "Detection" section
- `docs/SPEC.md` "Template Detection" section
