# Full reviews: An agent moves its own job ahead

Plan: [2026-11-26-agent-moves-own-job-ahead.md](2026-11-26-agent-moves-own-job-ahead.md).
All seats ran on opus.

## Round 1 (body 023f39180101, 1316 words)

- **po**, approve-with-changes (27k, 35 s). 1 high: no Outcome measure or rollback (accepted: `## Measure`). 2 medium: journal refusals (accepted: `promote_refused` rows); the waiting job never says its id (accepted: `slot run` id line).
- **tui**, approve-with-changes (36k, 37 s). 2 high: queue line over 80 columns (accepted, with a test); the waiting agent never sees its id (accepted). 3 medium: `try:` lines on refusals, streams and lock-wait notice and usage line, `"queue":[]` when slots are off (all accepted).
- **unix**, approve-with-changes (30k, 47 s). 2 medium: rename then journal lets a signal escape the budget (accepted: row first); exit codes and streams (accepted: 1/2/74/75). 1 low: `waiting: N` vs listed lines (accepted: `gone` lines).
- **backend**, approve-with-changes (39k, 70 s). 4 medium: head nanos minus 1 leaves no room (accepted: minus 1 s); `Status` must not change (accepted); verification gaps (accepted); 2 low: fallible append (accepted, merged with unix 1), journal scan size (accepted).
- **architect**, approve-with-changes (50k, 108 s). 1 high: `pid-seq` is not unique across pid namespaces (accepted: find by inode, `ambiguous`). 3 medium: liveness by pid (accepted: by flock), rotation would reset the budget (accepted: Decision log and holds-until), ADR `choice` text (accepted).
- **lang:rust**, approve-with-changes (51k, 131 s). 1 high: the concurrency test passes without the fix (accepted: promote to 1). 4 medium: refusal tests, no dependency, crate-private ticket record and named return type, session env isolation in the integration test (all accepted). 2 low: inode identity (merged with architect 1), slots-off `queue` (merged with tui 5).

## Round 2 (body d6ceb08b6073, 1882 words)

Every round-1 finding answered resolved. New findings, all medium or low, carried into implementation:

- **architect**, approve (59k, 56 s). Low: the Measure's off switch has no mechanism.
- **po**, approve-with-changes (36k, 44 s). Medium: the off switch has no setting. Low: Phase 3 should file the 7-day measure ticket.
- **tui**, approve-with-changes (47k, 54 s). Medium: number only live tickets, `gone` lines show `-`; medium: an `off` refusal; low: `ambiguous` text names both jobs.
- **backend**, approve-with-changes (55k, 70 s). Medium: the picture predates the inode and row-first edits; medium: no off switch is built; low: no test for append-ok/rename-fails; medium: the listing's try-lock opens with `O_CREAT` and can recreate a renamed ticket.
- **lang:rust**, approve-with-changes (62k, 64 s). Medium: the same `O_CREAT` race; low: the budget test should include `promote_refused` rows; low: the off switch.
- **unix**, approve-with-changes (41k, 59 s). Medium: the same `O_CREAT` race, and `slots` would need write access.

## Codex final review (body d6ceb08b6073)

- **codex**, approve-with-changes. High: a cut-short best-effort journal line can swallow the `promoted` row after it (applied, as a record split at each `{"at_ms":` start; fail-closed on any malformed record was rejected because one damaged line would disable promotion for good). Medium: sample the clock under the lock, not before the 10 s lock wait (carried).

## Backend delta on the Codex edit (body a849198ca7b4, 1939 words)

- **backend**, approve-with-changes (58k, 25 s). The split holds. Medium: a missing journal must count zero moves (carried; the code treats `NotFound` as zero). Low: the fragment test should also check that the fragment is not counted (carried).
