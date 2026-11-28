---
status: active
branch: fix/hsa-t23
repos: [amont]
adrs: [ADR-0022]
plan-id: p-01M8FH2S8V6ZHEEEV379DP74KM
---
# An off switch for slot promote

## Review panel
👉 **Decide:** none — approve if a host key `greengate.hostPromote` plus an off-only env variable is the right rollback lever.
📍 amont · 2 phases · 1 release · next: Phase 1. Panel: architect, backend, lang:rust, po, tui, unix (opus).
**Changed by review:** env can only turn promotion off; refusal text cut to 80 columns with `set by:`; relais-facing `promote_from_host`, crate publish and the 2026-12-03 release deadline.
**Verdicts:** round 2: 6 approve-with-changes, all applied; backend's last 2 mediums carried (live-check row excluded from the Measure; one warning for an invalid value); words 1036 → 1380 (×1.33, fixes and wrong statements); remove 0, merge 0.
📄 Full reviews: [2026-11-27-promote-off-switch.reviews.md](2026-11-27-promote-off-switch.reviews.md)

## Goal

`greengate slot promote` can be turned off without reinstalling an older
greengate. The t13 plan's `## Measure` says that above 1.5× p95 `waited_ms`
promotion "is turned off (refuses everything)", but 5.2.0 builds no switch;
the only rollback is 5.1.1. This adds the switch, and folds in the plan's
low review findings that t13 left deliberate.

## Picture

```
slot promote <id> --reason r
  usage error ............................ exit 2
  slots off (hostSlots 0) ................ exit 1 (as today)
  hostPromote resolves to 0 .............. refused (off), exit 1,
        env GREENGATE_HOST_PROMOTE=0      journal: promote_refused limit=off
        or git config --global            (only when the slot dir exists)
        greengate.hostPromote 0
  else ................................... the crate's rules, as today
```

The off check sits after usage and slots-off, before the crate's own rules.

## Non-goals

- No change to t06's rules (own session, head, 3 moves, 20 min floor).
- `slot run`, `slots` and the queue are untouched; only a move is refused.
- relais has no promote command yet (t24); it takes the crate's `Refused::Off`
  when it does.
- No per-repository switch: this is a machine decision (see Decision log).

## Code review

- amont: backend, lang:rust, unix, tui

## Behaviour

**The switch.** A host key, resolved like `greengate.hostSlots`:

- `git config --global greengate.hostPromote 0` turns promotion off for the
  machine (`1`, or unset, is on). `--system` works too; a value in a
  repository's own config is ignored with the existing warning.
- `GREENGATE_HOST_PROMOTE=0` turns it off for one process. The environment can
  only turn it off: `1` never overrides a host `0` (it warns), so a session
  cannot force promotion back on. An invalid environment value warns and falls
  to the next source. An invalid git value (`false`, `off`) leaves promotion
  on, so its warning says so and names `0`: `promotion stays on; to turn it
  off: git config --global greengate.hostPromote 0`. `slot promote` prints
  `resolve_promote`'s warnings through `host_slots`' `say_once` (which already
  prints the sizing warnings), so no warning shows twice. When git cannot list config scopes the key reads as unset
  (on), as `hostSlots` does; an environment `0` still refuses, and that
  warning names promotion, not sizing.
- It lives in the crate's `host` module (`PROMOTE_KEY`, `PROMOTE_ENV`,
  `OffBy` (`env`, `global`, `system`), `resolve_promote` (rows `None` when git cannot list scopes), and `promote_from_host()` beside `from_host`
  for a client with no rows of its own, as relais is), so relais and greengate
  read it once, the same way. `promote_in_waiting`'s doc says a caller checks
  the switch first; the t24 ticket gets the same note.

**What a refused promote prints.** Short lines, 80 columns at most, stderr, exit 1:

```
greengate: slot promote refused (off): promotion is off on this machine
  set by: git config --global greengate.hostPromote 0
  the job keeps its place; see: greengate slots
```

The `set by:` line is copyable text for the source: `GREENGATE_HOST_PROMOTE=0
in the environment`, or `git config --system greengate.hostPromote 0`. stderr
does not say how to turn it back on: the switch is a rollback set on a
measurement, so `docs/host-slots.md` carries the `--unset` and `unset
GREENGATE_HOST_PROMOTE` commands.

**What it journals.** One `promote_refused` row with `"limit":"off"` and
`"source"` (`env`, `global` or `system`), the
same shape as any refusal (session, id, tool, pid), so the t22 count sees the
switch's use. It is written only when the slot directory already exists.

**Crate.** `Refused::Off { source: host::Source }` (`limit()` `off`, `describe()` the
sentence above), added to the `#[non_exhaustive]` enum, and a public
`promote_off(dir, request, source)` that journals and returns it. The crate
reads no environment: the binary resolves the switch and calls it, before
the reason and session checks (an off switch refuses first).

**Low findings folded in** (from t13's deliberate list):

1. A `no-room` refusal gets two hints, in the age-floor hint's form:
   `see: greengate slots` and `try: greengate slot promote <id> --to N --reason <text>`.
2. The session variable list in the `no-session` hint is the crate's
   `SESSION_ENVS` (made public), not a copy.
3. `PromoteError`'s `Display` no longer includes the I/O cause; `source()`
   returns it and the command prints the chain once, on one line (`greengate: slot promote:
   <what>: <cause>`), so a reporter that walks
   `source()` no longer shows it twice.
4. A refused promote before the queue is touched (off, no reason, no
   session) no longer creates the slot directory: `safe_dir` runs after
   those checks, and their journal row is written only when the directory
   exists and passes the privacy check (`check_private`, which creates
   nothing); otherwise no row is written.
5. Tests for the `--` operand (taken as the id, a flag-looking operand is not
   a flag), the refusal hints, the 1 s lock-wait notice (a lock held 1.5 s by a
   thread: the callback runs once and the move succeeds), and the lock-busy
   exit 75 with its retry hint (the report function is split out and driven
   with `PromoteError::LockBusy`; the crate's busy path is driven with a
   shortened wait through a private seam).

## Phases

- [x] Phase 1 — the switch and the lows (the switch in its own commit, so it can ship alone): `crates/greenharness-slots/src/host.rs`,
  `crates/greenharness-slots/src/reorder.rs`, `crates/greenharness-slots/src/lib.rs`,
  `crates/greengate-runtime/src/slot_cmd.rs`, `crates/greengate-runtime/src/host_slots.rs`,
  `crates/greengate/tests/it/slot_run.rs`, `docs/configuration.md`, `docs/host-slots.md`,
  `crates/greengate/tests/common/mod.rs` (`pin_host_env` sets
  `GREENGATE_HOST_PROMOTE=1` beside `GREENGATE_HOST_SLOTS`, for cargo runs
  outside the repository), `.cargo/config.toml` (the same pin, forced, asserted
  in `cargo_pins_the_host_knobs_for_tests`, so a shell that exports
  `GREENGATE_HOST_PROMOTE=0` still passes the promote tests; a machine's
  `--global` `hostPromote 0` is already kept out by the sandboxed
  `GIT_CONFIG_GLOBAL`; an env `1` with no host `0` is silent, so the pin adds no
  stderr line; a `--system` `0` is carried, not pinned)
- [x] Phase 2 — release 5.6.0 (planned as a minor; cut as the major 6.0.0 with #543),
  publish `greenharness-slots`' matching version to crates.io (relais pins `"5.5"`
  from the registry), install, and point the t13 plan's rollback at the switch:
  `CHANGELOG.md`, `docs/plans/2026-11-26-agent-moves-own-job-ahead.md`
  (version numbers move with whatever is on main when it is cut)

## Decision log

- 2026-11-27 — Shape: a host key plus an environment variable, not an
  environment variable alone and not a repository key. Why: the rollback
  trigger (p95 above 1.5× the baseline) is a machine measurement, so the
  switch must persist across sessions and shells, which an environment
  variable does not; `git config --global` is the place the other machine
  keys already live (`hostSlots`, `hostCores`), with the same precedence and
  warnings, and the variable gives a one-run override and a test seam. A
  repository key was rejected: any clone could turn off or force on a
  machine-wide queue rule (as `hostSlots` already refuses).
- 2026-11-27 — Value form: `0` or `1`, as `hostSlots 0` is off. `resolve_promote`
  reads the git rows itself, not through `read_key`: `read_key` returns on any
  valid environment value (host.rs:190) and words its git warnings for sizing
  (host.rs:229). Then it reads `GREENGATE_HOST_PROMOTE`: `0` turns promotion off,
  `1` warns only when a host `0` exists and changes nothing. The off answer is an
  `OffBy` (`env`, `global`, `system`), a closed enum: no wildcard arm, and no
  `host::Source`, which admits values that cannot turn promotion off.
- 2026-11-27 — Off refuses before the reason and session checks, and does
  not need the slot directory; its row is written only when the directory
  exists, so turning it off never creates state.
- 2026-11-27 — Version: a minor bump (a new public key and enum variant, no
  break, `Refused` is non-exhaustive). Cut and installed before t22's
  2026-12-03 count, without waiting for siblings (those merged by then ride
  along); the number is whatever is next at the time.
- 2026-11-27 — The environment variable only turns promotion off, because
  agent sessions set their own environment and could otherwise undo a rollback
  (backend review).

## Verification

Observed on the final tree (the checks ran through `greengate slot run`; logs
in `/tmp/t23-all8.log`).

- `GREENGATE_HOST_PROMOTE=0 greengate slot promote 1-0 --reason r` → refused
  (off), exit 1, a `promote_refused` row with `"limit":"off"`. Observed: the
  built binary in `promote_off_refuses_by_name_from_each_source`: exit 1,
  stderr `refused (off): promotion is off …` and `set by: GREENGATE_HOST_PROMOTE=0`,
  empty stdout, row `"limit":"off","source":"env"`.
- `hostPromote = 0` in a scratch `GIT_CONFIG_GLOBAL` → the same with `set by:
  git config --global …`, row source `global`. Observed in the same test. With
  `GREENGATE_HOST_PROMOTE=1` and a global `0`: still exit 1, one warning "does
  not turn promotion back on". With `hostPromote = 1`: the crate's rules apply
  (`refused (not-queued)`). Three `off` rows in all.
- Crate tests: `cargo test -p greenharness-slots` 118 passed (precedence,
  invalid values, repository-only value, no-scopes, warning width, `Refused::Off`
  text for all three sources, `promote_off` journals and moves nothing, no
  directory made by a refusal, lock wait said once, `LockBusy`, I/O cause once).
- The lows, observed: `slot_cmd` tests 9 passed (`--` operand, hints for
  no-room, no-session and off, `LockBusy` → exit 75 with `retry:`, I/O error →
  exit 74 with the cause once); integration `a_bad_promote_creates_no_slot_directory`
  passed (no directory made, `-- 7-7` taken as the job); `host_slots` 12 passed
  (the cargo pin asserts `GREENGATE_HOST_PROMOTE=1`).
- `greengate --test it slot_` 22 passed. `cargo fmt --check` clean.
  `cargo clippy --workspace --all-targets -- -D warnings` fails on `origin/main`
  with rustc 1.100 in `crates/greengate-runtime/src/posix_signal.rs`
  (`suspicious-runtime-symbol-definitions`), a file this change does not touch;
  with that one lint allowed it is clean. Not fixed here: a follow-up.
- Post-install check (Phase 2, 2026-11-28, installed greengate 6.0.0, real
  journal): `GREENGATE_HOST_PROMOTE=0 greengate slot promote 999999 --reason
  "t23 live check"` → expected refused (off), exit 1, one `off` row. Observed:
  `refused (off): promotion is off for this process` and `set by:
  GREENGATE_HOST_PROMOTE=0 in the environment`, exit 1, and one row
  `"outcome":"promote_refused","limit":"off","source":"env"`. Without the
  variable, the same call gave `refused (not-queued)`. The config key, with
  `GIT_CONFIG_GLOBAL` pointing at a scratch file holding `hostPromote = 0`,
  printed `refused (off): promotion is off on this machine` and `set by: git
  config --global greengate.hostPromote 0`, exited 1, and wrote a row with
  `"source":"global"`. The 6.0.0 CHANGELOG carries the switch (#541). The workspace-wide suite ran in the pre-push gate,
  not here.

## Measure

At t22's count (2026-12-03): `promote_refused` rows with `limit` `off` while
no host `hostPromote` is set. A row with `source` `env` or `global` that
nobody set is a false off: revert to the previous release. Rows for id
`"999999"` (a string) on 2026-11-28 are t23's live checks (`off`/`env`,
`not-queued`, `off`/`global`): count with `select(.id != "999999")`.

## Outcome

Shipped in greengate 6.0.0: tag v6.0.0 on 1eb9b1c3, release PR #551, cut by
the topo-spotter-rename t28 session together with its breaking #543, not as
5.6.0. `greenharness-slots` 6.0.0 is on crates.io (published
2026-11-28 15:20 UTC). It is installed on this machine and live-checked (see
Verification). The t13 plan's Outcome names the switch as the rollback.
relais pins `greenharness-slots = "5.5"` (5.5.0 in its lock on 2026-11-28)
but has no promote path of its own. t24 makes relais jobs movable through
`greengate slot promote`, which checks the switch, so the switch covers every
move.

## Implementation review

- implementation-review: approve-with-changes (round 1), approve (delta 2); fixed `OffBy` wildcard arms, renamed tests, env wording test; 106k + 55k + 41k tokens.
- backend: approve-with-changes, then approve; fixed no-scopes warning on both readers; precedence documented.
- lang:rust: approve-with-changes, approve-with-changes, then approve; `OffBy` closed enum, `PromoteAnswer`, lock test on the seam.
- unix: approve-with-changes, then approve. tui: approve-with-changes, then approve; warnings fit 80 columns as printed.
- deliberate: `promote_in`, `promote_in_waiting`, `promote_with` and `refused_now` still spell out the nested result type (low, rust); the `promote_in_waiting` doc does not yet say a caller checks the switch first (follow-up). For t24 this is moot: it moves relais jobs through `greengate slot promote`, which checks it (2026-11-28).

<!-- panel: repos=amont adds=contract reviewers=architect,backend,lang:rust,po,tui,unix body-sha=d2ed1e84e5f5 -->- Phase 2 close-out (2026-11-28, docs only): implementation-review approve-with-changes over 3 rounds. Fixed: Phase 2 records, the duplicated body, the relais gap (t25 dropped, since relais moves jobs only through `slot promote`), and the live-check rows excluded in the Measure. Profiles: backend and tui approve-with-changes, then approve; lang:rust, unix and architect approve. Deliberate: stream and exit code of the control call not recorded (unix low); the switch is a caller's duty in the crate API (architect low; inbox follow-up).
