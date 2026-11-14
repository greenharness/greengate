---
canonical: greenharness/greenpiste:docs/plans/2026-11-14-greenpiste-renames-its-data-names.md
phases: [1]
status: done
branch: feat/greenpiste-data-reads
---
# greenpiste renames its data names (pointer)

This repository's slice of greenpiste's 3b-2 plan (Phase 1, readers read
both): greengate `attest.rs` reads each provenance payload from
`<config>/greenpiste/provenance`, and from the old `amont-agent` directory
only when it is missing there (named once per push); its message names
greenpiste. `greenharness-quiet` skips both temp prefixes and treats both
`tool` values as one tool. The old reads go at the programme census.

## Code review
- amont: backend, lang:rust (unix, tui, architect left out: two readers gain a fallback and one message names greenpiste; lang:typescript left out: no TypeScript file)

## Verification

- `cargo test -p greengate-runtime -p greenharness-quiet --lib -- provenance payload sweep same_tool greenpiste`
  → 14 passed, among them: each payload is read from the new dir first, then
  the old (neither, old alone, new root present with the payload in the old,
  both, an unreadable new entry never falls back); greenpiste and
  amont-agent entries are one tool in the sweep; both temp prefixes are
  kept fresh and swept stale. `cargo clippy --all-targets -D warnings` clean.
- The first push rehearsal failed `the_canon_refusal_fits_in_80_columns`
  (it asserted the old message text); fixed, `cargo test -p greengate-runtime
  --lib -- attest::` passes.
- The release and its install here are tracked in the canonical plan's
  Phase 1, not in this slice.

## Implementation review

- implementation-review: approve-with-changes, approve, then two short deltas after the push rehearsal caught a stale message test (fixed) and a one-letter fixture name (renamed); last: approve-with-changes, its fmt doubt checked clean (`cargo fmt --check`).
- backend: approve-with-changes then approve; the root-level choice lost payloads left in the old dir, now per-payload, NotFound-only, named once (57k/59 s, 26k/9 s).
- lang:rust: approve-with-changes twice; constants documented one by one; deliberate: an unreadable payload stays silent as before this branch, and the private tuple return stays (45k/31 s, 27k/18 s).
- The relais run was salvaged: its candidate held only greenharness-quiet; attest.rs and its tests were written by hand.
