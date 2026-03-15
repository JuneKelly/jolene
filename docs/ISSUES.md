# Issues

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
