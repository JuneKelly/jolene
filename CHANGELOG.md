# Changelog

All notable changes to Jolene are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Jolene is in alpha — breaking changes can occur in any release.

---

## [Unreleased]

### Breaking changes for bundle authors

#### `jolene.toml`: `[package]` table renamed to `[bundle]`

The manifest table that describes your bundle has been renamed:

```toml
# Before
[package]
name = "my-tools"
...

# After
[bundle]
name = "my-tools"
...
```

This affects all `jolene.toml` manifests. Bundles using the old `[package]`
header will fail to install with a parse error.

Similarly, the optional URL and prefix fields move from `[package.urls]` and
`package.prefix` to `[bundle.urls]` and `bundle.prefix`.

#### Template context: `jolene.package.*` renamed to `jolene.bundle.*`

Content files using the bundle name or version via the template context must
be updated:

```text
# Before
Provided by {~ jolene.package.name ~} v{~ jolene.package.version ~}.

# After
Provided by {~ jolene.bundle.name ~} v{~ jolene.bundle.version ~}.
```

Referencing `jolene.package` in a template will now produce a hard error at
install time. All other template context (`jolene.resolve()`, `jolene.prefix`,
`jolene.target`, `jolene.vars.*`) is unchanged.

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
