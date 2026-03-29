# Jolene: Known Gaps and Shortcomings

A comprehensive catalogue of design gaps, missing safeguards, UX shortcomings,
and architectural concerns in jolene's current design. Items already listed in
the spec's "Future Work" section (version pinning, per-project installs,
dependency resolution, Windows support, etc.) are not repeated here unless
they intersect with a broader concern.

---

## 1. Security and Trust Model

### 1.1 No content verification

Jolene installs content directly into AI tool config directories —
`~/.claude/commands/`, `~/.claude/skills/`, etc. These files are instructions
that coding agents follow. There is no signing, checksum verification, or
trust mechanism of any kind. A user running `jolene install --github
unknown/bundle` is trusting that repository to inject arbitrary instructions
into their AI tools.

The attack surface is significant: a malicious command or skill can instruct
an AI agent to exfiltrate code, run destructive shell commands, or manipulate
project files — all under the guise of a helpful plugin.

**Impact:** High. The content jolene installs has direct influence over AI
agent behaviour.

**Possible mitigations:**
- Content signing (bundle authors sign, jolene verifies).
- Checksum pinning in a lockfile.
- A review/confirmation step showing content before install.
- A curated registry with vetting.

### 1.2 Always tracks HEAD — no commit pinning

Bundles are cloned and updated by pulling the default branch. There is no
mechanism to pin a bundle to a specific commit, tag, or branch. This means:

- A bundle author can push new content at any time, and `jolene update` pulls
  it in without review.
- A compromised repository (e.g. stolen credentials, force-push) silently
  delivers malicious content on the next update.
- There is no way for a user to say "I trust version X of this bundle" and
  stay on it.

The spec's "Future Work" section mentions version pinning via git tags, but
the gap is broader than versioning — it's about the absence of any form of
update consent.

**Impact:** High. Updates are implicit trust decisions with no user
confirmation.

**Possible mitigations:**
- Pin to a commit SHA in the state file; require explicit `jolene update
  --accept` to advance.
- Show a diff of content changes before applying an update.
- Support `--ref` / `--tag` / `--commit` flags on install.

### 1.3 No pre-install content preview for remote bundles

`jolene contents --github owner/repo` clones the repository before the user
can inspect its contents. For native bundles, the clone happens as a
prerequisite to reading the manifest. There is no way to inspect what a
remote bundle contains without first downloading it.

**Impact:** Medium. The clone itself is low-risk (it's just a git fetch), but
it means the user has already committed to downloading before they can decide
whether to trust the content.

**Possible mitigations:**
- Fetch the manifest only (sparse checkout or raw file fetch via GitHub API)
  for the preview step.
- Clearly separate "fetch metadata" from "install".

### 1.4 No audit trail for updates

`jolene update` pulls new content, re-renders templates, adds/removes
symlinks, and updates the state file. None of these changes are logged. There
is no diff output, no record of what the previous commit was, and no way to
review what changed after the fact.

If an update introduces problematic content, the user has no tooling to
identify what changed or when.

**Impact:** Medium. Makes debugging update-related issues difficult and
undermines trust in the update process.

**Possible mitigations:**
- Log updates to a structured log file (old commit, new commit, symlinks
  added/removed, content diffs).
- Print a summary of changes during `jolene update` (new/removed/modified
  content items).
- Store the previous commit SHA in state so rollback is possible.

---

## 2. UX Gaps

### 2.1 No dry-run mode

`install`, `update`, and `uninstall` all mutate the filesystem and state file
immediately. There is no `--dry-run` or `--plan` flag to preview what would
happen before committing.

This is particularly concerning for:
- `install`: users can't see which symlinks will be created or which conflicts
  exist without triggering the actual install.
- `update`: users can't see what content changed upstream without applying it.
- `uninstall --purge`: irreversibly deletes the cloned repo.

**Impact:** Medium. Users must trust the tool to do the right thing with no
preview step.

**Possible mitigations:**
- Add `--dry-run` to all mutating commands. Print the planned actions without
  executing them.

### 2.2 No rollback

If `jolene update` pulls content that breaks something, the only recovery
path is to uninstall the bundle and reinstall it. But reinstalling fetches
HEAD again — the broken version. There is no way to revert to the previous
state.

The state file records the current commit but not the previous one. The
cloned repo's git history is available in theory, but jolene provides no
tooling to check out an older commit or restore a previous installation
state.

**Impact:** Medium. A bad update requires manual git intervention in the
store directory to recover.

**Possible mitigations:**
- Record the previous commit SHA in state during updates.
- Add `jolene rollback <bundle>` to revert to the previous commit.
- At minimum, print the old and new commit SHAs during update so the user
  has the information needed for manual recovery.

### 2.3 Prefix is immutable after install

The prefix applied during installation is locked into the state file.
Changing a prefix requires a full uninstall and reinstall cycle. This loses
any continuity — if other tools or workflows reference the prefixed names,
they all break temporarily during the transition.

**Impact:** Low-medium. Prefixes are a one-time decision for most users, but
the inability to change them without disruption is a rough edge.

**Possible mitigations:**
- Add `jolene rename-prefix <bundle> --prefix <new>` that atomically updates
  symlinks and state.
- Or document a migration path clearly.

### 2.4 `--var` type inference is fragile

The `--var key=value` flag infers the type of the value: `true`/`false` become
booleans, numeric strings become integers or floats, everything else is a
string. This means:

- A user who wants the string `"42"` as a value has no way to express it if
  the declared type is string — `--var count=42` becomes an integer.
- A user who wants the string `"true"` has no way to force string
  interpretation.
- There is no quoting or escaping mechanism to override inference.

The only workaround is `--vars-json '{"key": "42"}'`, which is non-obvious
and verbose for a single value.

**Impact:** Low-medium. Affects a narrow set of cases, but the failure mode
(type mismatch error with no obvious fix) is confusing.

**Possible mitigations:**
- Add a quoting convention (e.g. `--var key='"42"'` for explicit strings).
- Add `--var-string key=value` that always treats the value as a string.
- Document the `--vars-json` workaround prominently.

### 2.5 No discovery mechanism

There is no `jolene search`, no registry, no way to browse available bundles.
Users must already know the exact `owner/repo` of a bundle they want to
install. Marketplace support helps for repos that have a catalog, but
discovering those marketplace repos is itself an unsolved problem.

**Impact:** Medium. Limits adoption and makes the ecosystem harder to grow.
A tool that can't be discovered can't be used.

**Possible mitigations:**
- A curated registry (even a simple GitHub-hosted JSON index).
- `jolene search <query>` that queries the registry or GitHub topics.
- A community list of known bundles in the documentation.

### 2.6 `doctor` reports but doesn't fix

`jolene doctor` identifies broken symlinks, missing clones, orphaned
symlinks, and orphaned rendered directories. But it provides no `--fix` flag
to repair or clean up the problems it finds. The user must manually resolve
each issue.

**Impact:** Low-medium. Doctor is diagnostic-only, which is useful but
incomplete. Users who encounter issues still need to understand the internal
store layout to fix them.

**Possible mitigations:**
- Add `jolene doctor --fix` that removes broken symlinks, cleans orphaned
  state entries, and deletes orphaned rendered directories.
- At minimum, print actionable commands the user can copy-paste.

---

## 3. Architectural Concerns

### 3.1 Target detection heuristic is fragile

Jolene auto-detects targets by checking whether their config root directory
exists (`~/.claude/`, `~/.config/opencode/`, `~/.codex/`). This heuristic
has several failure modes:

- A directory left over from an uninstalled tool triggers installation to a
  tool the user no longer uses.
- A directory created for other purposes (e.g. `~/.codex/` for an unrelated
  tool) causes false positives.
- The user has no way to permanently exclude a target without using `--to`
  on every command.

**Impact:** Low-medium. The `--to` override exists, but the default behaviour
can surprise users.

**Possible mitigations:**
- Allow a global config file to exclude targets permanently.
- Require a marker file inside the config root (e.g. check for
  `~/.claude/settings.json` rather than just `~/.claude/`).

### 3.2 Advisory file locking only

Jolene uses `flock(2)` for concurrency control, which is advisory — it only
works if all processes agree to check the lock. A manual edit to `state.json`,
a second jolene process compiled without locking, or any other tool modifying
the state file will not be blocked.

Additionally, the `state::load()` function and `StateLock::acquire()` are
independent — nothing in the type system prevents loading, mutating, and
saving state without holding the lock.

**Impact:** Low in practice (single-user CLI tool), but the locking guarantees
are weaker than they appear.

**Possible mitigations:**
- Make `state::load()` only accessible through `StateLock` (e.g.
  `StateLock::acquire_and_load()` returns both the lock guard and the state).
- Document that `state.json` must not be edited manually.

### 3.3 Symlinks use absolute paths with no self-healing

All symlinks are fully expanded absolute paths. If the user's home directory
changes (renamed user account, moved home directory, different machine),
every symlink breaks. `jolene doctor` will report them but can't fix them
(see 2.6).

There is no mechanism to detect that paths have shifted and offer to
re-create symlinks with updated paths.

**Impact:** Low for most users, high for anyone who migrates systems or uses
jolene across multiple machines.

**Possible mitigations:**
- Add `jolene doctor --fix` that detects path-shifted symlinks and recreates
  them.
- Store the home directory in state and detect when it changes.
- Consider relative symlinks where possible (though this adds complexity).

### 3.4 Single flat state file with no schema versioning

All bundle state lives in one `state.json` file with no version field, no
schema identifier, and no migration strategy. Changes to the state format
rely on serde defaults and implicit compatibility (e.g. `source_kind`
defaulting to `"github"` for legacy entries).

As the tool evolves and the state format changes, this implicit migration
approach becomes increasingly fragile. A breaking change to the state format
would require manual intervention or a one-off migration tool.

**Impact:** Low now, grows over time as the format evolves.

**Possible mitigations:**
- Add a `version` field to the state file.
- Implement explicit migration logic that runs on load when the version is
  older than expected.

### 3.5 Lock file permissions inconsistency

The state file is created with mode `0600` (owner read/write only), but the
lock file (`~/.jolene/.lock`) is created via `File::create()` with the
default umask (typically `0644`). The lock file's existence and timing can
leak information about when jolene commands are running.

**Impact:** Low. Minor inconsistency, not a practical security risk for most
users.

**Possible mitigations:**
- Set `0600` on the lock file for consistency.

---

## 4. Design Inconsistencies

### 4.1 Two-tier content model: native vs marketplace

Native bundles and marketplace plugins have fundamentally different
capabilities:

| Feature                  | Native bundles | Marketplace plugins |
|--------------------------|:--------------:|:-------------------:|
| Templating               | yes            | no                  |
| Variable overrides       | yes            | no                  |
| Manifest-declared content| yes            | no (filesystem scan)|
| Prefix from manifest     | yes            | no                  |

This creates a confusing split. A plugin author who wants templating or
variable overrides must maintain a `jolene.toml` and ask users to install via
the native path — even if the plugin also lives in a marketplace.

**Impact:** Medium. The two modes feel like different tools sharing one CLI.
Plugin authors face a choice between marketplace discoverability and native
features.

**Possible mitigations:**
- Support an optional `jolene.toml` in marketplace plugin directories for
  authors who want native features.
- Or explicitly document that marketplace mode is a compatibility shim and
  native bundles are the primary format.

### 4.2 Marketplace content discovery is implicit and unguarded

Native bundles install only what's declared in `[content]`. Marketplace
plugins install everything found via filesystem scan — any `.md` file in
`commands/`, any directory with `SKILL.md` in `skills/`, any `.md` in
`agents/`.

This means:
- A stray or draft `.md` file in `commands/` gets installed silently.
- There is no allowlist — the plugin author has no way to exclude files from
  installation except by not placing them in those directories.
- A file accidentally committed to the plugin directory becomes an installed
  command.

**Impact:** Medium. The implicit discovery model trades safety for
convenience.

**Possible mitigations:**
- Respect a `.jolene-ignore` or similar exclusion file in plugin directories.
- Warn when installing content that isn't referenced by a `plugin.json` if
  one exists.

### 4.3 `source_kind` legacy default is a migration hack in the format

The `SourceKind` enum defaults to `GitHub` when the field is missing, baked
into the serde deserialization. This handles entries that predate the field,
but it means:

- Any corruption or accidental deletion of `source_kind` silently produces a
  GitHub entry, even if the bundle was installed from a local path or URL.
- The default is a data-layer concern masquerading as a serialization
  convenience.

**Impact:** Low. Only affects corrupted or manually edited state files. But
the silent misclassification is concerning.

**Possible mitigations:**
- Treat a missing `source_kind` as an error after a transition period.
- Add an explicit migration step that backfills `source_kind` for old entries.

---

## 5. Missing Operational Features

### 5.1 No garbage collection

Over time, the jolene store accumulates:
- Orphaned clones in `repos/` from bundles that were uninstalled without
  `--purge`.
- Stale `rendered/` directories from bundles that no longer have templated
  content or were uninstalled.
- Old rendered copies that are no longer referenced after an update changed
  which items are templated.

`jolene doctor` reports orphaned rendered directories, but there is no
command to clean them up. Orphaned clones are not reported at all unless they
still have state entries.

**Impact:** Low. Disk space accumulates slowly. But for users who install and
uninstall frequently, the store grows without bound.

**Possible mitigations:**
- Add `jolene gc` or `jolene clean` to remove unreferenced repos and rendered
  directories.
- Have `uninstall` clean up rendered directories by default (not just with
  `--purge`).
- Have `update` clean up rendered directories for items that are no longer
  templated.

### 5.2 No reproducible installations / no lockfile

There is no mechanism to capture the exact set of installed bundles (with
commit SHAs) and reproduce that installation on another machine. Each
`jolene install` fetches HEAD, and the resulting state depends on when the
command was run.

This means:
- Teams cannot share a locked bundle configuration.
- CI/CD environments cannot install a deterministic set of bundles.
- Moving to a new machine requires manually re-running each install command
  and hoping the bundles haven't changed.

**Impact:** Medium. Limits jolene's usefulness in team and CI contexts.

**Possible mitigations:**
- A `jolene.lock` file that records bundle sources and commit SHAs.
- `jolene install --lockfile jolene.lock` to reproduce an exact installation.
- `jolene export` to generate a lockfile from the current state.

### 5.3 No update notifications

There is no way to know whether installed bundles have upstream changes
without running `jolene update`. There is no `jolene outdated` or
`jolene check` command to compare local state against remotes.

**Impact:** Low. Users who want updates run `jolene update`. But the lack of
a check command means users must choose between updating blindly and not
knowing whether updates exist.

**Possible mitigations:**
- Add `jolene outdated` that fetches remote refs without pulling, and reports
  which bundles have new commits.

---

## Summary

The most significant gaps fall into two categories:

**Trust and safety (1.1, 1.2, 1.4, 4.2):** Jolene installs content that
directly controls AI agent behaviour, with no verification, no update review,
and no content allowlisting for marketplace plugins. This is the highest
priority area to address.

**Operational confidence (2.1, 2.2, 5.2):** No dry-run, no rollback, and no
reproducible installs make it difficult to use jolene with confidence in
anything beyond a personal, experimental context.

The remaining items are quality-of-life and architectural hygiene issues that
matter for long-term maintainability but are not urgent blockers.
