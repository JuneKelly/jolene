# Jolene

Jolene is a plugin manager for coding agent commands, skills, and more.

Supported targets are opencode, codex, and claude code.

Currently, this project is in the exploration phase, and subject to change.

## Key Files

- @docs/SPEC.md - specification
- @README.md - project readme
- @docs/TEMPLATING.md - documentation on the templating system
- `docs/proposals/` - design proposals

## Workflow

Use `bd` (beads) for task tracking, if available. `bd` should be initialised in
stealth mode (`bd init --stealth`).

## Working on Documentation

This project is spec-driven: `docs/SPEC.md`, `docs/TEMPLATING.md`, `README.md`,
and design proposals under `docs/proposals/` are primary artifacts that
cross-reference each other heavily — CLI flags, state fields, section/layer
numbers, and summary tables that duplicate details from the body.

- **Keep references in sync.** After renaming a flag/field, renumbering
  sections, or deleting a section, `grep` for every reference (including derived
  summary tables and cross-links), update them together, then re-grep to confirm
  nothing dangles.
- **Design vs. implementation.** Proposals describe *what and why* — behaviour,
  rationale, contracts. Keep the *how* (file lists, function signatures, test
  plans) out of them or in a clearly separate section; duplicated implementation
  detail drifts out of sync with the body.
- **Converge before editing.** When the user is workshopping a design, agree on
  the approach before making edits. Prose is cheap to change; re-deriving intent
  is not.
- **Consistency pass after big edits.** After lifting/moving content or deleting
  blocks, verify qualifiers survived the move, code fences balance, and no
  reference points at removed content — don't wait to be asked.

## Technology and Implementation

This project uses `rust` to build a CLI program called `jolene`.
