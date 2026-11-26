---
status: active
branch: fix/hsa-t15
repos: [amont, relais]
adrs: [ADR-0022]
plan-id: p-01M8CXC3FW74JP75SNS6T1CYKN
---
# Held marker names a live lease

## Review panel
👉 **Decide:** none — approve if a live-lease marker, accepted in both forms first and refused bare last, is the safe three-step rollout.
📍 amont, relais · 3 phases · 6 releases · next: Phase 1. Panel: architect, backend, lang:rust, po, tui, unix (opus).
**Changed by review:** crate reader typed `Refusal` and `#[non_exhaustive]`; rollback trigger and order; t15 stays open for its one-day count.
**Verdicts:** round 2: 6 approve (backend's last 3 findings applied, then approve on the final body); 2 carried; words 1955 → 2400 (×1.23); remove 0, merge 0. codex: not installed.
📄 Full reviews: [2026-11-26-held-marker-names-live-lease.reviews.md](2026-11-26-held-marker-names-live-lease.reviews.md)

## Picture

```
holder: slot run, hook gate, relais    exports held:<lease id>
                 |
child: greenharness_slots::held_marker()
  held:<id>, record live, flock taken  -> Held: no second queue
  unset or empty                       -> Absent: queues
  bare held (P3), 1, dead id           -> Refused: 1 stderr line,
                                          marker_refused row, queues

P1 readers accept both -> P2 writers emit held:<id> -> P3 bare refused
(each phase: greengate release+install, then relais release+install)
```

The picture shows the one reader both tools call, its three answers, and the three rollout steps.

## Goal

`GREENHARNESS_HOST_SLOT` with any non-empty value lets a command skip the host
slot queue in greengate, and exactly `held` does in relais (t10). An inline
`GREENHARNESS_HOST_SLOT=held greengate slot run -- cargo test` therefore
skips the queue unseen. After this plan, the marker a real holder exports is
`held:<lease>`, and the one reader in `greenharness-slots` admits a child
without queueing only when that lease is live. A stale or hand-set marker
queues, says so in one stderr line, and leaves a `marker_refused` row in
`waits.jsonl`.

## Non-goals

- greenbelay's `heavy-build-unqueued` keeps its presence check of the hook's
  own environment (t10: it never reads slot files).
- No secret in the marker. The holder records stay 0644; a copied live lease
  id still passes. The check stops careless and habitual use, not a
  determined agent (t10's threat model).
- No ancestry check: relais's coordinator keeps a group's tokens without being
  its ancestor (t10).
- The vendored greengate `^4.11` in duro-app, duro-design-system and greenrack
  is not bumped up front; one is bumped (its own PR) only if the phase-3 gate
  sees a 4.x writer from it. Their hooks run their own old binary, which accepts any
  non-empty marker, so their nested checks never reach the new reader.
- No change to the admission rule, the stall rule, promote, or the cap.

## Code review
- amont: backend, lang:rust, unix, architect (tui left out: one new stderr line, no table or prompt)
- relais: backend, lang:rust, unix, architect (tui left out: one stderr line, no table or prompt)

## Behaviour

### The reader (crate `greenharness-slots`, phase 1)

- `Lease::id(&self) -> &str` (the existing `<pid>-<seq>`) and
  `Lease::marker(&self) -> String` (`held:<id>`), and `held_value(id)`.
- `pub enum Marker { Absent, Held, Refused { value: String, reason: Refusal } }` (both `#[non_exhaustive]`, so a later reason is not a major bump for relais; the value comes from `to_string_lossy`; for the stderr line, control characters are escaped with `char::escape_default` first, then the escaped text is cut with `chars().take(64)`; the journal row takes the raw value cut to 64 through the crate's JSON `escape()`, so `jq` parses it) and
  `pub fn held_marker() -> Marker` (reads `HELD_ENV` and `dir()`), with the
  testable `held_marker_in(dir, value: Option<&OsStr>) -> Marker`:
  - unset or empty: `Absent` (no line, no row; as today, empty reads as not held);
  - bare `held`: `Held` while the crate's policy accepts it (phases 1–2),
    `Refused` once it does not (phase 3). The policy is a private constant fed
    to an internal reader that takes it as a parameter, so tests cover both;
  - `held:<pid>-<seq>` (digits, one dash; anything else is `Refused`): `Held`
    only when (1) a `holders/slot-N.who` record carries that `lease` id and its
    pid is alive, and (2) that `slot-N` is locked, probed with the read-only
    `is_locked` (it never creates a file) under a non-blocking `try_lock` of the
    `admission` lock, opened without create: when `admission` does not exist, no
    lock is tried and nothing is created (nothing can be admitting), and (2) is
    probed directly. When the admission lock is busy, (1) suffices, as
    `status_in_on` already allows. The directory is checked with
    `check_private`, never `safe_dir` (which creates). Any I/O error, a missing
    or foreign directory: `Refused`;
  - anything else (`1`, a path, a dead or unknown lease): `Refused(value)`.
  - Cost: up to `cap` small reads, one `kill(pid,0)`, up to 2 flocks.
- `Refused` carries a reason: `bare`, `malformed`, `unknown_lease` (no record
  names it), `dead_pid`, `slot_free`, `io`. A refusal naming a lease that is in
  the records (`dead_pid`, `slot_free`) is a legitimate child failing the
  check; the others are forged or stale. The day count reads that off the
  journal.
- `pub fn record_marker(dir, tool, value, outcome, reason)` appends one row
  with no `waited_ms` (wait readers select `has("waited_ms")`, as with
  `released` rows), starting with `{"at_ms":` (the promote budget scan splits
  on it):
  `{"at_ms":…,"tool":"greengate","pid":…,"ppid":…,"session":…,"marker":"<value, escaped, ≤64 chars>","reason":"unknown_lease","outcome":"marker_refused"}`.
  `ppid` is `std::os::unix::process::parent_id()` (std only). From phase 2 an
  accepted bare `held` writes the same row shape with the outcome
  `marker_bare` (no `reason`), once per process: the phase-3 gate counts it.
- `pub fn refused_line(bin, value, reason) -> String`, two lines like the
  other queue messages: `<bin>: GREENHARNESS_HOST_SLOT=<value> names no live
  lease (<reason>); queueing`, then one indented hint of at most 80 columns by
  reason: `bare` -> `  bare held is refused since 5.5; its holder must be 5.4 or later`;
  `dead_pid` or `slot_free` -> `  its holder has ended; see: <bin> slots`; others
  -> `  unset it; only a slot holder sets it`. The value is escaped and cut as
  above before it reaches the line or the row.

### greengate (phases 1–3)

- `host_slots.rs` `nested()` and `slot_cmd.rs` (`nested`) call
  `held_marker()`. On `Refused`, and only once sizing says slots are on and
  supported (so the command really queues), the line goes to stderr and the
  row to the journal, once per process, and the command queues exactly as an
  unmarked one; the exit status stays the child's (a refusal adds no exit
  code). The verdict is computed once per process (`OnceLock<Marker>`) and
  every `nested()` call reads it, so a lease ending midway cannot admit some
  checks and queue others. Slots off or unsupported: no line, no row.
- Phase 2 writers: `slot_cmd.rs` exports `lease.marker()` for a granted
  lease; a nested `slot run` (marker accepted) leaves the inherited value
  untouched instead of re-setting bare `held` (a new `Launch::Inherit`).
  `hooks/common.rs` (both `before_spawn` sites) export the marker of the lease
  this thread holds, or of the parked lease when a light check runs between
  two heavy gates, through one helper `host_slots::held_env()`.

### relais (pointer plan, phases 1–3)

- Phase 1: `runner/engine.rs` (whose `heavy_with` runs at 6 call sites per
  run, so the once-per-process guard is a crate `MarkerLog` value the caller
  holds in one `static` and passes to `record_marker`; tests build their own,
  so parallel tests share no flag) calls `greenharness_slots::held_marker()`
  (`dir()`, as greengate) instead of `ancestor_holds_slots(== "held")`; when
  the run's host queue is on, a refusal prints relais's line and writes the
  `marker_refused` row (tool `relais`), then the run queues. `Cargo.lock` moves to crate 5.3.x, which also
  brings crate 5.2's ticket records (half of t24's question).
- Phase 2: `verify/heavy.rs` writes `held:<lease>` for its own lease (the id
  read before `keep()` moves it; `reuse_kept` yields the kept lease's id for a
  setup's later steps), and passes the inherited, accepted marker through
  unchanged under an ancestor. Accepted gap: a run under
  `[resources] shared_host_slots = "<dir>"` (none configured on this machine)
  holds its lease in another directory, so a greengate child reading `dir()`
  is refused and queues on the default queue, never behind its own parent.
- Phase 3: `Cargo.lock` to crate 5.5.x; requirement floor `"5.5"`.

### Fixtures (phase 1, both repos)

amont's `.cargo/config.toml` force-sets `GREENHARNESS_HOST_SLOT=held` for
every test binary; it becomes a forced empty marker plus
`GREENGATE_HOST_SLOTS=0` (slots off), and `cargo_pins_the_host_knobs_for_tests`
asserts that. `tests/common/mod.rs` `pin_host_env` sets slots off instead of
`held`; its unpinned path removes `GREENGATE_HOST_SLOTS` too (the variable
beats git config). `tests/timing.rs:553` sets slots off. `it/slot_run.rs`'s
nested test passes the real marker the holder exported (captured to a file by
the holder's command), not `held`. relais's `.cargo/config.toml` gets the same
change (`force = true`), and `cargo_pins_the_host_slot_for_tests` follows.
Moving them first keeps fixture refusals out of the real journal the day
count reads.

## Phases
- [ ] Phase 1 — readers accept both forms, refusal line and row, fixtures moved, ADR amended; greengate 5.3.0 released and installed, then relais (crate 5.3 reader) released and installed: `crates/greenharness-slots/src/lib.rs`, `crates/greengate-runtime/src/host_slots.rs`, `crates/greengate-runtime/src/slot_cmd.rs`, `.cargo/config.toml`, `crates/greengate/tests/common/mod.rs`, `crates/greengate/tests/timing.rs`, `crates/greengate/tests/it/slot_run.rs`, `adr/0022-waits-end-on-a-stalled-line.md`, `adr/HEADS.md`, `docs/host-slots.md`, `docs/configuration.md`, `CHANGELOG.md`; relais `crates/relais/src/runner/engine.rs`, `crates/relais/src/verify/heavy.rs`, `crates/relais/src/verify/heavy/tests.rs`, `.cargo/config.toml`, `Cargo.lock`, `CHANGELOG.md`
- [ ] Phase 2 — writers emit `held:<lease>`; greengate 5.4.0 and relais released and installed: `crates/greengate-runtime/src/slot_cmd.rs`, `crates/greengate-runtime/src/hooks/common.rs`, `crates/greengate-runtime/src/host_slots.rs`, `crates/greengate/tests/it/slot_run.rs`, `CHANGELOG.md`; relais `crates/relais/src/verify/heavy.rs`, `crates/relais/src/verify/heavy/tests.rs`, `CHANGELOG.md`
- [ ] Phase 3 — gated on the phase-3 gate (Verification); bare `held` refused (`ACCEPT_BARE = false`); greengate 5.5.0 and relais released and installed; the post-install checks; t15 stays open until the one-day count of legitimate-child refusals is read and recorded in its Resolution, then closes: `crates/greenharness-slots/src/lib.rs`, `adr/0022-waits-end-on-a-stalled-line.md`, `adr/HEADS.md`, `docs/host-slots.md`, `CHANGELOG.md`, `docs/programmes/host-slot-admission/t15-build-held-marker-lease.md`; relais `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`

Each phase is its own branch from `origin/main` in each repository (amont
`fix/hsa-t15`, `fix/hsa-t15-p2`, `fix/hsa-t15-p3`; relais
`prog/host-slot-admission/t15-build-held-marker-lease` and `-p2`, `-p3`),
and ends with a `docs(plan): held-marker-names-live-lease, phase N` closing
commit. greengate is released first in each phase; the relais PR waits for
the crate on crates.io.

## Decision log
- 2026-11-26 — minor releases (5.3.0, 5.4.0, 5.5.0), because an
  environment variable's accepted values are not in
  `cli.compat.interface-is-public` (flags, subcommands, exit codes, `--json`);
  t10 counted zero hand-set uses in 7 days.
- 2026-11-26 — phase 1 already refuses everything but bare `held` and a live
  `held:<lease>` (so `=1` stops skipping greengate's queue now): no legitimate
  writer has ever written another value, and phase 2 then soaks the live check.
- 2026-11-26 — the refusal row carries no `waited_ms`, because wait readers
  select `has("waited_ms")`, and it carries a `reason`, because `ppid` cannot
  tell a legitimate grandchild (cargo → test binary → greengate) from a forged
  marker.
- 2026-11-26 — phase 3 is gated on observed writers, not on elapsed hours: a
  bare `held` still written (an old relais run, a 4.x greengate holder such as
  t14's greenrack worktrees) would be refused after the flip, and on the hook
  path wait to `greengate.timeout` behind its own parent. A 4.x writer seen
  in the gate holds the ticket.
- 2026-11-26 — relais is installed with the relais coordinator stopped while
  idle (t11 Phase 5's recipe), so no old in-flight relais keeps writing bare
  `held`.
- 2026-11-26 — the default on any failure is `Refused`, which queues, and on
  the hook path a refused legitimate child waits to `greengate.timeout`. So
  the reader is the one place to look when a deferral spike follows an
  install: the `reason` field names the failing condition. Rollback is the
  previous release of both tools, with `GREENGATE_HOST_SLOTS=0` for one run
  as the bypass. Trigger: the first `marker_refused` row with reason
  `dead_pid` or `slot_free` within 24 h of any phase's install. Order: relais
  never goes below its phase-1 release while greengate 5.4 or later is
  installed (relais 0.90.0 accepts only bare `held` and would queue behind a
  greengate parent), so greengate rolls back to 5.3 first.
- 2026-11-26 — greengate's own CI and fixtures run with slots off
  (`GREENGATE_HOST_SLOTS=0`), so the suites do not exercise the queue except
  in the slot fixtures that set their own scratch directory.
- 2026-11-26 — t10 named the slot directory variable
  `GREENHARNESS_HOST_SLOTS_DIR`; it is `GREENHARNESS_SLOT_DIR` (`DIR_ENV`).
- 2026-11-26 — risk since t18: a refused legitimate child on the hook path
  waits to `greengate.timeout`, not one 300 s stall (t10's bound). Phase 3 is
  therefore gated on the soak count below.
- 2026-11-26 — the relais lock jump to crate 5.3 carries crate 5.2's ticket
  records into relais; t24 is told in the Resolution.

## Verification
- Phase 1, unit (crate): a scratch dir with a held slot and a record with
  lease `L` → `held:L` is `Held`; `held:` + an unknown id, a dead pid's record,
  a record whose slot is free (admission lock free), `1`, `held:x y` →
  `Refused`; empty → `Absent`; bare `held` → `Held` (and `Refused` with
  `ACCEPT_BARE` off, a test seam); the refusal row parses as JSON with no
  `waited_ms`, starts `{"at_ms":`, and carries `ppid`.
- Phase 1, piloted on `target/debug/greengate` with a scratch 0700
  `GREENHARNESS_SLOT_DIR` and `GREENGATE_HOST_SLOTS=1`:
  `GREENHARNESS_HOST_SLOT=1 greengate slot run -- true` prints the refusal
  line once, writes one `marker_refused` row, and runs the command after a
  grant (`admitted` row); `GREENHARNESS_HOST_SLOT=held` (bare) writes no `admitted` or
  `marker_refused` row and prints no refusal line. `held_marker()` on a held
  lease with cap 8 takes under 1 ms (median of 100 calls, recorded). Also
  `GREENHARNESS_HOST_SLOT=$'x\e[2J'` prints its value escaped (no raw ESC byte
  under `od -c`), a 40-character multibyte value does not panic, and
  `=1 ... -- sh -c 'exit 7'` exits 7.
- Phase 1, integration (`it/slot_run.rs`, scratch 0700 dir, slots on): `=1`
  with two heavy checks gives exactly one stderr line, one `marker_refused`
  row and one `admitted` row; the same with `GREENGATE_HOST_SLOTS=0` gives no
  line and no row.
- Phase 1, fixtures: the greengate-runtime, greengate and relais suites,
  run once, add zero rows of any kind to the real `waits.jsonl` (line count
  before equals after), and pass with slots forced off; a grep of the real journal after the
  suite shows no `marker_refused` row from a test pid.
- Phase 2, integration: the nested `slot run` test passes with the captured
  `held:<id>` and stays under 2 s; a hook fixture's child sees
  `held:<id>` matching its holder record; relais's setup test asserts one
  `held:<id>` across all steps; two bare-`held` reads on one `MarkerLog` write
  exactly one `marker_bare` row with no `waited_ms` (crate unit test).
- Phase 3 gate, read before the flip and no earlier than 24 h and 20 accepted `held:<id>` markers (counted from the reader's acceptance path) after the relais phase-2 install: since that install, zero
  `marker_bare` rows and zero `marker_refused` rows whose `reason` is
  `dead_pid` or `slot_free`, apart from this plan's own probes (their pids
  listed); and `greengate slots --json` shows no holder whose `started_ms`
  predates that install. Either failing holds phase 3 until it clears; a 4.x
  writer seen in the gate is answered by bumping that repository's vendored
  greengate to 5.4 (its own PR), after which the gate is read again.
- Phase 3, unit: every phase-1 refusal case passes through `held_marker_in`
  with the shipped policy (no env mutation); `held_marker()` stays a
  one-line wrapper.
- Phase 3, installed binaries (also run literally as `amont slot run`):
  `GREENHARNESS_HOST_SLOT=1 greengate slot run -- true` and `GREENHARNESS_HOST_SLOT=held greengate slot run -- true` each print
  the refusal line and journal `marker_refused`; `greengate slot run -- sh -c
  'greengate slot run -- true'` writes one `admitted` row, not two. The
  one-day count of legitimate-child refusals (`dead_pid` and `slot_free`
  rows) is read at the phase-3 install plus 24 h and written into t15's
  Resolution before t15 closes (the ticket's own acceptance check).
- Every phase, before its install: `waits.jsonl` rows since the phase's
  first build carry no `marker_refused` or `marker_bare` row from a test
  binary (fixture leakage), and after the install the binary under
  `which greengate` / `which relais` prints the released version.

## Outcome

<!-- panel: repos=amont,relais adds=contract reviewers=architect,backend,lang:rust,po,tui,unix body-sha=2573b2bcc069 -->
