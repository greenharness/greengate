---
status: active
branch: feat/slots-journal-holders
repos: [amont, relais]
adrs: []
plan-id: p-01M82M36M0TFMZA41J97S90KWY
---
# The slot journal records holders and hold time

## Review panel
👉 **Decide:** approve the growth, 1,163 → 1,858 words (×1.60). Each addition answers a reviewer finding (6 high, 1 blocking); otherwise ask for a smaller plan.
📍 amont, relais · 4 phases · 2 releases · next: Phase 1. Panel: backend, rust, tui, unix, architect, product owner (opus), codex.
**Changed by review:** `held_ms` uses a monotonic `Instant` and pairs with the grant by `granted_ms`. Each lease's id lives on `Lease`, so dedupe is per lease and `Holder` is unchanged. Older-writer rows and censored hold times are named for t08.
📄 Full reviews: [2026-11-23-slot-journal-records-holders.reviews.md](2026-11-23-slot-journal-records-holders.reviews.md)
**Verdicts:** final body: 5 approve, tui approve-with-changes (low); codex approve-with-changes. 14 carried into implementation; remove 0, merge 0.

## Picture

```
 callers (hook, slot run, relais)    greenharness-slots (shared crate)
 ────────────────────────────────    ────────────────────────────────
 Waiter::poll → Granted(lease) ────▶ Lease keeps who + when   (NEW)
 record_wait_named("admitted") ────▶ row + holders,
                                       holder_jobs            (NEW)
                                       from holders/*.who
 runs, parks, hands, renames           (live pids, no lock)
 drop(lease) ──────────────────────▶ Drop frees slots, then
                                       appends "released"
                                       row with held_ms       (NEW)
                         │
                         ▼
          /tmp/greenharness-slots-501/waits.jsonl → t08
```

The callers stay as they are; the crate adds the two counts to every wait
row and one release row per dropped lease, all in the same journal.

## Goal

Programme `host-slot-admission`, ticket t09. Today `waits.jsonl` has one row per
acquisition. Each row has the wait, but not how busy the host was, nor how long
the slot was then held. Without that, t01's `k = 4` cannot be checked on a
workday and t07's setup hold time cannot be measured. After this change every
row carries `holders` and `holder_jobs`, and every dropped lease leaves one
release row with `held_ms`. Both are written inside `greenharness-slots`, so
amont (hooks and `slot run`) and relais write them the same way without changing
any caller.

## Non-goals

- No change to admission (t11), the relais setup lease (t12) or the reorder
  (t13). The journal only observes.
- No imputed jobs. A holder whose record has no `jobs` adds 0 to `holder_jobs`;
  t01's "unknown counts as `parallelism(1)`" belongs to the rule in t11.
- No reader or report over the journal; t08 does the measuring.
- No rotation or size cap for `waits.jsonl`. Release rows roughly double its
  line count, which is accepted here.
- No new public field on `WaitContext` or `Queue`. amont builds `WaitContext`
  without `..default()` (`host_slots.rs:600`), so a field there would be a
  breaking change.

## Code review

- amont: backend, lang:rust, unix (tui left out: only journal rows change)
- relais: backend, lang:rust (unix, tui left out: no shell or screen change)

## Behaviour

**Wait rows** (`record_wait_named`, and so `record_wait` and `record_wait_jobs`)
gain two fields after `"cores"`:

- `"holders"`: the number of slots held when the row is written. It counts the
  live `holders/slot-N.who` records: the pid is alive, and hidden `.tmp` files
  are skipped. A weight-2 lease counts 2.
- `"holder_jobs"`: the sum of `jobs` over distinct leases. Each new holder
  record carries a private `"lease"` id (`<pid>-<seq>`), shared by every slot
  of one lease and kept by `Lease::rename`; occupancy dedupes by it, not by
  whole-record equality, so two sibling leases from one cloned `Holder` count
  apart and a lease seen mid-rename counts once. The id lives on `Lease`
  only: `Holder` gains no field at all (a private one breaks struct literals
  and `..Default::default()` downstream, a semver-major change, and alters
  its derived `PartialEq`). The id is minted in `poll_with` from a
  process-wide counter and written by one private record writer that both
  the grant and `rename` call, never re-read from the old file. The grant's
  write goes through a hidden temp file and a rename, as `rename` does, so a
  scan never reads it half-written. `Holder::to_json`, `greengate slots
  --json` and `read_holders`'s equality dedupe stay as they are; occupancy
  reads the id with `raw`. A record without the id falls back to equality.
  `cargo semver-checks` against 4.12.0 reports no major change.

Every writer records after the lease exists. So an `admitted` or `handed` row
counts the new acquisition itself, and its own `weight` and `jobs` are in the
row. Rows that end `timeout`, `unqueued` or `deferred` count only the others.
The doc comment says this, and a test pins it. A t01-style analysis subtracts
the row's own `jobs` from admitted rows. Same-process siblings are counted:
they are real load.

The count reads the records only. It takes no lock, so it never contends with
an admission. A slot held by a client that writes no record (amont ≤ 1.61) is
not counted. No such client is installed.

**Release rows.** `Lease`'s `Drop` appends one line after it frees the slots:

```
{"at_ms":…,"tool":"relais","pid":…,"session":"…","command":"cargo test","weight":1,"jobs":2,"granted_ms":…,"held_ms":84211,"outcome":"released"}
```

- `tool`, `session`, `command` and `jobs` are the holder's at grant time.
  `held_ms` runs from grant to drop on a monotonic `Instant` kept on the
  lease, never a difference of two `now_ms()` (a clock step back would
  underflow a `u128` inside `Drop`, and tests fake `probe.now_ms`).
  `granted_ms` is the wall-clock grant (`now_ms()`, not `probe.now_ms`).
- There is one row per `Lease`, not one per acquisition row. greengate parks
  one lease across several gates (`admitted`, then `handed` rows). The pairing
  is by `pid` and `tool`: a release row belongs to the first `admitted` row
  at or after its `granted_ms`. That row's `at_ms` is written after the
  grant, so the two are never equal. A process holding several leases at
  once can pair ambiguously, and relais's release `session` is its `run_id`
  while its wait rows carry the env session; t08 notes both.
- `"outcome":"released"` keeps every row groupable by `outcome`. The release
  row has no `waited_ms`, `load1` or `cores`, so a reader that sums `waited_ms`
  must filter it out: `jq -cR 'fromjson? | select(has("waited_ms"))'`. It
  also skips any later row kind with no wait, and `-R` with `fromjson?` skips
  a writer's half-appended last line instead of stopping there. This filter goes
  in `docs/host-slots.md` and in t08 as a copy-pasteable line.
- Best-effort like every journal write: a failed write never fails the drop.
- A process killed with its lease held leaves no release row; the kernel frees
  the flock. So does a handled SIGHUP/SIGINT/SIGTERM: `land` exits with
  `process::exit(128+sig)` (`land_detach.rs:408`, also `cluster/detached.rs:88`),
  which skips every `Drop`, `ParkScope`'s included, and SIGTERMs its group.
  An unpaired `admitted` row that carries `holders` means an interrupted
  holder, so `held_ms` is censored toward short holds: t08 reports the
  unpaired count beside every hold-time statistic. An `admitted` row without
  `holders` comes from a writer older than 4.13 (relais before Phase 4, a
  hook binary baked into `node_modules`): occupancy unknown, never 0, and
  left out of pairing. The acceptance says "every dropped lease".

The module doc (`lib.rs:17`, "one line per acquisition") becomes: "one line per
acquisition, and one per released lease."

## Phases

- [ ] Phase 1 — crate: `crates/greenharness-slots/src/lib.rs`
  - `Lease` gains a private `release: Option<Release>` built in `poll_with`
    from the waiter's `dir`, holder, weight, an `Instant` and `now_ms()`
    taken at the grant. `Release` holds `PathBuf`, `String` and `Instant`
    and derives `Debug` (as `Lease` does), so `Lease` stays `Send`, as the
    `PARKED` static needs. `Drop` keeps its order (records, locks, count), then
    writes the row.
  - A private `occupancy(dir) -> (usize, u32)` scans `holders/`. Its write goes
    into `record_wait_named`'s format line.
  - The tests:
    - an admitted row with one other holder at weight 2 gives `holders 3` and
      `holder_jobs` equal to the two records' sum, deduped;
    - a timeout-style row written with no lease counts only the others;
    - an empty directory gives `"holders":0,"holder_jobs":0`;
    - two leases from one cloned `Holder` count their jobs apart, and a
      weight-2 lease with one record renamed counts its jobs once;
    - a lease granted through `poll_with` with a faked probe clock and held
      across a 200 ms sleep appends exactly one `released` row with
      200 ≤ `held_ms` < 2000, `granted_ms` near now, and the grant's
      command, also after a `rename`;
    - a lease dropped without a writable directory does not panic;
    - the old-field test (`record_wait_keeps_its_old_fields…`) still passes;
    - every wait outcome written by `record_wait_named` carries `waited_ms`,
      which the `has("waited_ms")` filter relies on.
- [ ] Phase 2 — amont readers: `crates/greengate/tests/it/busy_defers.rs`,
  `crates/greengate/tests/it/cluster_gates.rs`,
  `crates/greengate/tests/it/slot_run.rs`
  - Assertions that count or index wait lines filter on
    `outcome != "released"`; in busy_defers.rs the filter goes into the
    shared `waits()` helper (:976), with a second helper returning all rows
    for the release-row test.
  - One test asserts that a pre-push which takes and hands a parked slot
    leaves exactly one `released` row for that chain.
- [ ] Phase 3 — docs and release: `docs/host-slots.md`, `CHANGELOG.md`,
  `docs/programmes/host-slot-admission/t08-workday-measure.md`,
  `docs/programmes/host-slot-admission/t09-journal-holders-and-hold-time.md`
  - The host-slots doc gets the new fields, the release row, and two
    copy-pasteable `jq` one-liners: waits only (`select(has("waited_ms"))`)
    and release-to-grant pairing by `pid` and `tool`.
  - t08 gets these "Look at" notes:
    - release rows are excluded from acquisition counts;
    - admitted rows include themselves in `holders`;
    - the release-to-chain pairing; an unpaired admitted row with
      `holders` is an interrupted holder; a row without `holders` is from
      an older writer, left out of occupancy, pairing and the k = 4 check;
    - order rows by `at_ms`, not line order: a release row is appended
      after its flocks are freed, so a successor's row can come first.
  - t09 is closed with a Resolution. It states that the release row covers dropped leases only; interrupted holders leave none (narrower than the ticket's "every lease").
  - Minor release 4.13.0 through `tag-release`.
  - Read the crates.io job's `conclusion`: the token lasts 90 days.
  - Install on this machine.
- [ ] Phase 4 — relais (separable: Phase 3 closes t09 for amont without waiting on it): `Cargo.lock`, `crates/relais/src/verify/heavy/tests.rs`
  - `cargo update -p greenharness-slots`, from 4.0.0 to 4.13.0. That brings
    12 minor releases of admission and width behaviour into relais; read the
    crate's CHANGELOG 4.1–4.13 and run relais's whole suite, not only this
    test. 4.13.0 carries `bound_width` as on main when it is cut.
  - `a_queued_command_gets_the_slots_share…` also asserts `"holders":1` and,
    after `drop(lease)`, one `released` row.
  - Pointer plan, PR, merge, relais release through `tag-release`, install.

## Decision log

- 2026-11-22 — Fields come from the crate, not from callers, because both
  tools then write them the same way with no API change (a minor bump).
- 2026-11-22 — An admitted row counts itself, because all four writers record
  after the grant. Reading the state at write time is honest, and the row
  carries its own `weight` and `jobs` to subtract.
- 2026-11-22 — The release row uses `"outcome":"released"`, not a separate
  `event` key, so t08's grouping by `outcome` stays total.

## Verification

- Phase 1: `cargo test -p greenharness-slots` → the new tests pass, and they
  go red with the change reverted → <result>.
- Phase 2: `cargo test -p greengate --test it -- busy_defers cluster_gates
  slot_run` → green, with the release-row assertion → <result>.
- Piloted, amont, before the push: in the worktree, set
  `GREENHARNESS_SLOT_DIR=<scratch>` and run `greengate slot run -- sleep 2`
  while a second `slot run -- sleep 5` holds a slot →
  - the second admitted row has `"holders":2`;
  - each run leaves one `released` row, with `held_ms` in [2000, 2300] and [5000, 5300].
  → <result>
- After install: `/tmp/greenharness-slots-501/waits.jsonl` gains a real
  greengate row with `holders`, and a `released` row → <result>.
- After the relais install: the same for a `"tool":"relais"` row → <result>.

## Measure

After each install, 24 hours of `waits.jsonl`:
- 100% of wait rows from 4.13 writers carry `holders` and `holder_jobs`.
- Every `released` row has `held_ms` ≥ 0 (under 1 ms floors to 0); unpaired admitted rows are counted.
- Growth stays under 3× the baseline of 232 rows per 12 hours (t08); above
  that, rotation becomes its own ticket.
- Roll back (revert, next patch release) on a panic in `Drop`, or when the
  p95 of `waited_ms` on `admitted` rows with `holders` of 1 exceeds twice
  its 2026-11-21/22 baseline, measured before install.

## Outcome

<!-- panel: repos=amont,relais adds=contract reviewers=backend,lang:rust,tui,unix,architect,po,codex body-sha=ee1562f70030 -->
