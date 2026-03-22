# Changelog

All notable changes to Jolene are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Jolene is in alpha — breaking changes can occur in any release.

---

## [0.1.7]

### Breaking changes for bundle authors

The terminology used by the jolene project has changed:

- "packages" are now "bundles"
- the project is described as a "plugin manager"

#### Template context: `jolene.package.*` renamed to `jolene.bundle.*` (breaking)

Content files using the bundle name or version via the template context must
be updated:

```text
# Before
Provided by {~ jolene.package.name ~} v{~ jolene.package.version ~}.

# After
Provided by {~ jolene.bundle.name ~} v{~ jolene.bundle.version ~}.
```

Referencing `jolene.package` in a template will produce a hard error at
install time. All other template context (`jolene.resolve()`, `jolene.prefix`,
`jolene.target`, `jolene.vars.*`) is unchanged.

### Deprecated

#### `jolene.toml`: `[package]` table renamed to `[bundle]`

The manifest table that describes your bundle has been renamed. The old
`[package]` header is still accepted but will print a deprecation warning:

```
Warning: jolene.toml uses deprecated [package] table — rename it to [bundle]
```

Update your manifest:

```toml
# Before (deprecated)
[package]
name = "my-tools"
...

# After
[bundle]
name = "my-tools"
...
```

Similarly, `[package.urls]` becomes `[bundle.urls]` and `package.prefix`
becomes `bundle.prefix`.

### Changed

- Jolene is now described as a **plugin manager** rather than a package manager.
  The term "bundle" is used for native installable units (git repos with a
  `jolene.toml`); "plugin" is reserved for marketplace items.

- `state.json` key renamed from `"packages"` to `"bundles"`. Existing state
  files using the old key are **automatically migrated** the first time a
  mutating command (`install`, `uninstall`, `update`) is run. No manual action
  required.

- All CLI help text, error messages, and command descriptions updated to use
  "bundle" terminology.

- Documentation (README, SPEC, TEMPLATING) updated throughout.
