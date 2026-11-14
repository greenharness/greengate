---
canonical: greenharness/greenpiste:docs/plans/2026-11-14-greenpiste-renames-its-data-names.md
phases: [1]
status: active
branch: feat/greenpiste-data-reads
---
# greenpiste renames its data names (pointer)

This repository's slice of greenpiste's 3b-2 plan (Phase 1, readers read
both):  backend, lang:rust (unix, tui, architect left out: two readers gain a fallback and one message names greenpiste; lang:typescript left out: no TypeScript file):greengate attest.rs reads <config>/greenpiste/provenance before the old directory and names greenpiste in its message; greenharness-quiet skips both temp prefixes and accepts both tool values. The new name is read first, the old one only when the new is
absent; the old reads go at the programme census.

## Code review
- amont
