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

**Location:** SPEC.md lines 101 vs 106

**Problem:** The CLI syntax for `--pick` is contradictory between two sections:

Line 101 shows:

```
jolene install --marketplace --github <org/repo> --pick <plugin>[,<plugin>...]
```

This indicates comma-separated values in a single argument.

But line 106 says:
> `--pick <name>` — Select plugins from the marketplace catalog. **Repeatable.** Comma-separated. Required when `--marketplace` is set.

The word "Repeatable" implies you can use `--pick foo --pick bar`, but the syntax shows comma-separated format. These contradict each other.

**Impact:** Users won't know whether to use `--pick foo,bar` or `--pick foo --pick bar`.

**Resolution needed:** Decide on one format and update both the syntax line and the description.

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

**Recommendation:** Either:

1. Add `jolene.resolve("name", "command")` validation — error if that content type isn't supported by `jolene.target`
2. Document that `resolve()` returns the installed name regardless of whether the content type is installed
3. Skip running templates for unsupported content types entirely

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
