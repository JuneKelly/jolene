# Todo

## `find_package` uses display string as identity key

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
