# Full reviews (reference): Admission counts holders; stalled waits defer

Plan: [2026-11-23-admission-counts-holders-stalled-waits.md](2026-11-23-admission-counts-holders-stalled-waits.md).
Body as presented: 1760 words (sha 25131e7a99e2). Final: 2327 words (sha 7220cf1a32a9).
Every seat ran on opus.

## Round 1 (sha 25131e7a99e2)

- **backend**, approve-with-changes (32k tokens, 39 s). Findings:
  1. medium: relais architect seat; accepted.
  2. medium: one clock for the Stall marks; accepted.
  3. medium: derive LOAD_PER_JOB; accepted (t01's arithmetic).
  4. medium: grep the `--on-timeout` callers; accepted.
  5. medium: no release before t14; accepted.
  6. medium: a journal end reason; accepted (`end` field).
  7. medium: verification for the hook, `now` and Phases 4–6; accepted.
  8. low: a relais ADR record; merged into architect 1.
- **po**, approve-with-changes (27k, 19 s). Findings:
  1. high: post-release measure and rollback; accepted.
  2. high: gate Phase 5 on a recount; rejected, because the Destination asks
     for relais as well.
  3. medium: a hook pilot; accepted.
  4. medium: the `--on-timeout` callers; accepted.
- **architect**, approve-with-changes (44k, 53 s). Findings:
  1. blocking: a relais record replacing `execution.resource-budgets`; accepted.
  2. high: overrun against `>=`; accepted.
  3. medium: one Σjobs function; accepted.
  4. low: the decision takes effect at install; accepted.
- **lang:rust**, approve-with-changes (33k, 35 s). Findings:
  1. high: the pilot cannot show a moving line, and the stall window needs an
     env override; accepted.
  2. high: saturating arithmetic; accepted.
  3. medium: 5.0.0 or `#[non_exhaustive]`; rejected (see the Decision log).
  4. medium: no version bump before Phase 4; accepted.
  5. medium: a clock-seam test per table row; accepted.
  6. medium: overrun persistence; answered (in-memory).
  7. low: keep the greenbelay corpus cases; kept.
- **tui**, approve-with-changes (41k, 47 s). Findings:
  1. high: the deferral text names its cause; accepted.
  2. medium: two exit-75 messages; accepted.
  3. medium: parser, usage strings and `resolve_busy`; accepted.
  4. low: a two-line removal message; accepted.
- **unix**, approve-with-changes (37k, 48 s). Findings:
  1. high: `now` from the environment only; accepted.
  2. medium: `--now` as a boolean flag; accepted.
  3. medium: a deprecation note for `--on-timeout exit`; accepted.
  4. low: tell a stall from a ceiling; accepted, through the `end` field.

## Round 2 (sha 99ce172af1db)

- lang:rust approve (43k, 15 s); backend approve-with-changes (49k, 25 s);
  po approve (36k, 14 s); architect approve (54k, 27 s); tui
  approve-with-changes (48k, 15 s); unix approve-with-changes (45k, 16 s).

## Codex (final review)

Verdict rework. Findings:
1. blocking: a pre-stage deferral leaves no completion, so land refuses
   `evidence-missing`. Verified at `dispatch.rs:1580` and `dispatch.rs:1878`.
   Accepted as `Completion::Deferred` (an hour), with ADR-0022 also replacing
   `hooks.gates-before-connection`.
2. high: relais's coordinator queue shares `WaitKind::Admission`; accepted
   (the overrun is for the host-slot wait only).
3. medium: an I/O error after the queue is joined runs unqueued; carried.

Backend re-ran on sha 7220cf1a32a9: approve-with-changes (53k, 21 s), with
findings 12 (medium) and 13 (low) carried.

## Carried into implementation

- backend 9: label the `now` check as Phase 2.
- backend 10, po 6, architect 6: the rollback is to reinstall greengate 4.13
  only, note it in ADR-0022, and leave relais on the 4.14 crate.
- backend 11: name the relais ADR file in Phase 5.
- backend 13: Phase 2 names the `Completion::Deferred` work.
- po 5: a Phase 5 stall criterion.
- tui (round 2) 1: `run now:` keeps the waiting command, and
  `skip the queue (the host is busy):` offers `GREENGATE_ON_BUSY=now`.
- unix (round 2) 5, medium: the waiter's own marks use a monotonic clock, and
  only `admitted-at` is wall-clock. The idle time is the minimum of the
  per-clock terms, and a test covers a forward jump.
- unix (round 2) 6: a bad `GREENHARNESS_SLOT_STALL_S` value complains once and
  falls back to 300; an invalid `GREENGATE_ON_BUSY` lists `wait, defer, now`.
- lang:rust (round 2): flag an inline `GREENGATE_ON_BUSY=now` in greenbelay
  (a follow-up); a test where `Wait::tick` is called after the clamp; the
  relais record lands before the relais release; `semver-checks`, or an
  equivalent check, before the publish.
- codex 3 and backend 12, medium: an I/O error after the queue is joined ends
  the wait as a stall does, never unqueued.
