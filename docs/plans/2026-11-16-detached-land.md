---
status: done
plan-id: p-01M7JDWHTXSAA7MEMGETF65JSW
branch: feat/land-detach
repos: [amont, amont-agent]
adrs: []
---
# B2: a detached land with a receipt

## Review panel
👉 **Decide:** approve the growth, 1,053 → 1,936 words (×1.84): every addition answers a blocking or high finding (pinned sha, write order, lock hand-off, `--wait` alone); or ask for a smaller plan.
📍 amont, amont-agent · 2 phases · 3 releases (greengate, greenpiste, grh) · next: Phase 1. Panel: backend, rust, tui, unix, architect, product owner (opus).
**Changed by review:** the worker pushes only the sha judged at launch and never retries; one writer per receipt, liveness by an inherited `flock`; `--wait` stands alone and publishes nothing.
**Verdicts:** round 1: 1 rework, 3 approve-with-changes; round 2 and two deltas: all 6 approve on f97a50c20d50. 9 blocking or high fixed; mediums and lows carried into implementation.

## Picture
```
agent ──► greengate land --detach --json   (foreground, back in seconds)
            │ 1. records judged = HEAD; writes receipt status=running
            │ 2. spawns the worker; it takes the receipt and a lock
            │ 3. prints {status:detached, receipt, log, wait}; exit 0
            ▼
          worker: gates → push judged sha → receipt landed|refused|…
agent ──► Monitor: greengate land --wait <receipt>   (verdict, exit code)
```
The guard judges the dry-run before the launch, as for a foreground land.

## Context

The time-to-done programme (decisions t04, t05; build B2) found that 201 of
203 gate commands moved to the background had already used the Bash tool's
10-minute maximum: lands really run past 10 minutes (slot queues, long
suites). A background run frees the turn but its result is unknown when the
call returns, which is why greenpiste refuses a background land
(`greenpiste-guard/src/land.rs:62-64`). t05 decided shape (a): a detached
land that returns at once and writes a receipt the agent follows with
Monitor. The commit gate does not change.

## Goal

- `greengate land --detach` returns in seconds with the receipt path; a
  worker runs the gates and the push and finalizes the receipt.
- greenpiste lets the detached form through its land guard, after the same
  dry-run, and its landing assertion treats a `running` receipt as not yet
  judgeable.
- greenpiste rule `land-detached` (advise) points a foreground land with a
  near-maximum timeout to `--detach` and Monitor.

## Non-goals

- The time the gates take (slot queues, suites): ADR-0014 and the
  proportional-ship programme own it.
- The commit gate, and any background `git commit`.
- Letting a foreground-shaped land run in the background (t05 shape b).
- Updating the `worktree-task` skill's F6 land step; a follow-up after the
  rule is observed.

## Code review

- amont: backend, lang:rust, unix (tui left out: no new screen, one stderr hint)
- amont-agent: backend, lang:rust (unix, tui left out: a hook rule and an assertion, no command-line flag or output)

## Behaviour
**greengate (`crates/greengate-runtime/src/land.rs`).**
- `--detach` joins the hand parser (`:294-308`); it combines with `--json`,
  `--tag` and `--force` as the foreground form does, and conflicts with
  `--dry-run` (usage, exit 2). A non-unix host refuses it (exit 2), as
  `spawn_detached` does (`rehearsal.rs:652`). The exit-code table
  (`:17-27`) gains `detached` (0).
- **Pinned sha.** The launcher records `judged` (HEAD, or the tag's
  commit) in the receipt. The worker pushes only that sha and ends
  `refused` if HEAD moved: commits made after the launch are never pushed
  unjudged. That includes a pre-push hook that fast-forwards HEAD: a
  detached land never takes the retry (`land.rs:580-604`) and ends
  `refused`, naming the new HEAD to judge and land again.
- **One writer at a time.** The launcher writes the receipt once, before
  the spawn: `<git-dir>/greengate/land/<started>-<pid>.json` with
  `status: running`, `pushed: ""`, `branch` (or the tag ref), `judged`,
  `log`, `started`. It never writes it again after a successful spawn. The
  worker's first act rewrites it `running` with its own `worker_pid`. A
  spawn failure finalizes it `refused` and exits 1. A launch is refused
  (exit 1) while another `running` receipt in the worktree has a live
  worker.
- **Logs.** The worker is spawned through `rehearsal::spawn_amont`
  (`rehearsal.rs:669`) with a hidden `land --worker --receipt <path>` plus
  the original flags. Its output goes to `<started>-<pid>.log` beside the
  receipt (spawn_amont truncates, so lands never share a log), pruned
  with it.
- **Writes are atomic.** Every receipt write goes to `<name>.json.tmp` in
  the same directory, then a rename, so `receipts_since` and
  `prune_receipts` (`land.rs:1128`) never read half a file.
- **Liveness.** The launcher takes an exclusive `flock` on `<receipt>.lock`
  before it writes the receipt, and hands that fd to the worker: the flag
  `FD_CLOEXEC` is cleared in the child only (a `pre_exec` `fcntl`, so no
  other spawn inherits it; `spawn_amont` gains an inherited-fd argument),
  and the worker gets the fd number as a hidden `--lock-fd N`. The worker
  never opens `<receipt>.lock` again (a new open is a separate file and
  would block on its own lock) and sets `FD_CLOEXEC` back as its first
  act, so no gate, push, hook or daemon it starts holds the lock after the
  worker dies. A
  `flock` belongs to the open file, so it stays held after the launcher
  exits: a receipt never reads `running` with its lock free while its
  worker lives. The launch check (scan for a `running` receipt whose lock
  is held, then write and spawn) runs under a short worktree-level
  `land.lock`, so two simultaneous launches cannot both pass. On SIGTERM, SIGINT or SIGHUP the worker finalizes
  `interrupted` (exit 128+n). A SIGKILLed worker leaves `running` with the
  lock free; that, not `kill -0`, marks it dead (pids are reused).
- **The worker always finalizes**, the early refusals included, where
  `write_receipt` returns today without a file (`land.rs:1086`). Every
  receipt, foreground or detached, gains `remote_before`, `remote_after`,
  `gates` (`owed`, `prestage`) and `exit`, matching `--json`
  (`land.rs:183-205`); `write_receipt` takes the path.
- **`greengate land --wait <receipt>`** polls every 2 s until the status
  is final or the lock is free with `running`. It prints the receipt's
  verdict line (≤ 80 columns, through `verdict_word`), or `worker <pid>
  died: see <log>; rerun: greengate land --detach`, and exits with the
  land's exit code (dead worker: 1).
- **Launch output.** With `--json`, stdout is one line
  `{"status":"detached","receipt":…,"log":…,"wait":"greengate land --wait '<receipt>'"}`.
  Without it, stdout is the `--wait` command alone (path single-quoted,
  pastes into zsh) and stderr ends `detached <short-sha> <branch>` and
  `not landed yet: wait on the receipt`.

**greenpiste (`crates/greenpiste-guard`).**
- `land.rs` `shape()` accepts `--detach` in the flag loop (`:106-133`),
  and `land --wait <path>` as a read-only call; `SHAPE` (`:38-39`) names
  `greengate land` and the detached form. The background refusal stays.
  `--detach` is not part of the dry-run memo key (`:170`).
- `land --wait` publishes nothing only alone: greengate refuses `--wait`
  with any other land flag (usage, exit 2), and greenpiste reads exactly
  `land --wait <one plain path>` as non-publishing; any other mix goes
  through `shape()` and is refused there. `push_target.rs` `resolve_for`
  (`:531-545`) returns `Push::DryRun` for it as for `--dry-run`, and
  `land_retry::examine` (`:42-51`) and `push_preflight` skip it, so no
  review, preview or retry check runs on a wait.
- `Receipt` gains optional `remote_after`, `exit`, `judged`, `worker_pid`;
  `read_receipt` still needs `branch`, `pushed`, `status` (a running
  receipt carries `pushed: ""`).
- `assertions/land_retry.rs` `judge` (`:67-101`): `running` gives
  `Verdict::Unknown("retry-check deferred: detached land still running")`,
  journalled, never a silent pass. The final receipt is the agent's to
  read with `--wait`; the worker's pinned sha keeps it to the judged
  commit.
- The version probe in `heavy_build_unqueued.rs` (`:193-228`) moves to a
  shared module that takes the clause's program; its comparison is a pure
  function, tested.
- New `rules/land_detached.rs`, stance advise, max deny: `examine` keys on
  a land without `--detach`, `--wait` or `--dry-run`; `confirm` fires in
  the foreground with `timeout` ≥ 540000 when greengate is 4.7.0 or later.
  The remedy is the call's own command with `--detach` added, ready to
  copy, and `greengate land --wait` under Monitor. Registered in
  `rules/mod.rs`, `src/rule_table.rs`, `corpus.rs`, with
  `tests/corpus/land-detached.cases`.

## Phases
- [x] Phase 1 — greengate: `--detach` with the pinned sha, the worker, the lock, `--wait`, the receipt fields and atomic writes. `crates/greengate-runtime/src/land.rs`, `crates/greengate-runtime/src/names.rs`, `crates/greengate/src/main.rs` (help), `crates/greengate/tests/it/land.rs`, `crates/greengate/tests/it/flows/land.rs`, `CHANGELOG.md`, `Cargo.toml`. Released as greengate 4.9.0 (#486, #488; 4.7.0 and 4.8.0 were cut by other branches first) and installed here as 4.10.0, which contains it.
- [x] Phase 2 — greenbelay (greenpiste until 6.0.0, #276): the guard reads `--detach` and `--wait`, the deferred assertion, the shared version probe, the `land-detached` rule. `crates/greenbelay-guard/src/land.rs`, `push_target.rs`, `rules/push_preflight.rs`, `assertions/land_retry.rs`, `rules/heavy_build_unqueued.rs`, `rules/land_detached.rs`, `rules/mod.rs`, `src/rule_table.rs`, `corpus.rs`, `tests/corpus/land-detached.cases`. Released as greenbelay 6.1.0 (greenbelay#279, #280); grh 0.9.0 (grh#49) pins greengate 4.10.0 and greenbelay 6.1.0; both installed. Live check 2026-11-17: the grh pin branch landed through `greengate land --detach --json`, then `greengate land --wait <receipt>` under Monitor: receipt `landed`, `pushed` = `judged` = remote, 21 s. A first launch ended `unreachable` (DNS failure on github.com, exit 3, nothing pushed), as the receipt said. The check found greenbelay 6.1.0 holding the `--wait` command greengate prints (path single-quoted); fixed in greenbelay#283, released in 6.2.0. The `land-detached` rule fired on a foreground tag land the same day.

## Decision log
- 2026-11-16 — Shape (a) per t05: a `running` receipt exists from the first second, so the assertion and the agent's wait both have a file to read.
- 2026-11-16 — The worker pushes the sha judged at launch, never a later HEAD (round 1, backend, blocking).
- 2026-11-16 — The launcher writes the receipt once, before the spawn; the worker takes it over (round 1, unix and rust, blocking): no race with a fast final write.
- 2026-11-16 — Liveness is a held `flock`, read by `greengate land --wait`; macOS has no `flock` command, so the wait is a greengate subcommand, not a shell loop.
- 2026-11-16 — Known gap: the hook cannot judge the final receipt of a detached land (PostToolUse fires at the launch); it journals the deferral, and the pinned sha bounds what can be pushed.
- 2026-11-16 — The skill's F6 step is not changed in this plan; the advise rule carries the advice first.
- 2026-11-16 — The receipt is a cross-repo contract that changes only by addition: `started`, `branch`, `pushed`, `status` stay required, and a status greenpiste does not know is `Unknown`, never a pass.
- 2026-11-16 — A dead worker's `--wait` exits 1, as Behaviour says; the died lines tell it from a refusal (round-1 implementation review; the carried "exit 3" is not taken).
- 2026-11-16 — Implementation was a relais run whose verification failed only on two relais.toml checks naming pre-rename crates (`amont-runtime`, `amont`); the candidate was committed by hand (relais decision `salvaged`). Fixing relais.toml changes this repo's trust hash, so it waits for the person's re-grant.

## Verification
- Phase 1, launch: in a fixture repo, `greengate land --detach --json` exits 0 within 5 s and prints `status: detached` with a receipt path; the receipt reads `running` with a `worker_pid`.
- Phase 1, finish: `greengate land --wait <receipt>` ends with exit 0; the receipt reads `landed`, its `pushed` equals `judged` and the remote branch sha (`git ls-remote`), with `remote_after` and `gates` filled.
- Phase 1, pinned sha: a commit made right after the launch is not pushed; the receipt ends `refused`, `ls-remote` unchanged.
- Phase 1, hook moved HEAD: a fixture pre-push hook that fast-forwards HEAD under `--detach` → receipt `refused`, `retried: false`, `ls-remote` unchanged.
- Phase 1, startup: 50 launches each followed at once by `--wait` → no died line; a second launch started together with one is refused.
- Phase 1, lock not leaked: `lsof <receipt>.lock` during a land with a gate suite lists only the worker; `kill -9` on the worker while a gate child (`sleep 600 &`) runs → `--wait` prints the died line within one poll, and the next launch is accepted.
- Phase 1, race: 50 early-refusal launches (no branch) → none left `running`; each ends `refused`.
- Phase 1, dead worker: `kill -9` on the worker → `--wait` prints the died line naming the log, exit 1, within one poll.
- Phase 1, two lands: two launches in a row in one worktree → the second refused while the first runs; after it ends, two logs, each intact.
- Phase 1, flags: `--detach --dry-run` → exit 2; foreground `land --json` on the fixture prints the same keys as before.
- Phase 2, guard: `greengate land --detach --json`, `--detach --tag v1` and `land --wait <path>` are read; `land --detach` in the background is refused as before.
- Phase 2, assertion: a `running` receipt journals `retry-check deferred`; a final `landed` receipt is judged as today; `greengate land --wait <path>` gets no push-guard decision and no `land-retry` line; `land --detach --wait x`, `land --wait x --force` and `land --wait x --tag v1` are refused through `shape()`, and greengate exits 2 on each.
- Phase 2, rule: a foreground `greengate land --json` with timeout 600000 on greengate 4.7.0 → advised, with the rewritten command; with timeout 120000, or greengate 4.6.0 → no finding.
- Phase 2, live: after install, one real `greengate land --detach` from a task worktree, then `--wait` under Monitor, ends `landed`.

Observed on tree 90bf8c50 (`cargo test -p greengate --features test-hooks --test it -- land_detach::`, 16 passed, 0 failed; the commit gate's full suite passed):
- launch, finish: `a_detached_land_returns_at_once_and_finishes_landed` → exit 0, receipt `landed`, `pushed` = `judged` = remote tip.
- pinned sha: `a_commit_made_after_the_launch_is_never_pushed` → refused, remote unchanged.
- hook moved HEAD: `a_hook_that_moves_head_is_refused_and_never_retried` → refused, `retried: false`, log says "pre-push hook moved HEAD", remote unchanged.
- startup: `an_immediate_wait_after_each_launch_never_reports_a_death` → 5 rounds, each exit 0, no died line. Short of the plan's 50 rounds.
- lock not leaked: `a_killed_worker_is_dead_within_a_poll_and_leaks_no_lock` → `kill -9` while the hook's `sleep 60` lives: died within 6 s, exit 1, next launch accepted. No `lsof` check was run.
- race: `an_early_refusal_leaves_no_receipt_running` → 5 rounds, none left `running`. Short of the plan's 50.
- dead worker: exit 1, died line names the log and a rerun with the original flags.
- two lands: `a_second_launch_while_a_worker_lives_is_refused_and_names_the_receipt` → second refused naming the receipt and `--wait`; two intact logs.
- flags: `--detach --dry-run` exit 2; `--wait` with any other flag exit 2; `--detach --tag v1 --force` exit 2; foreground `--json` keys unchanged.
- added by review: an annotated tag lands detached; a refused launch under `--json` prints JSON; a missing tag adds no receipt; a worker given fd 1 finalizes `refused`; SIGTERM → `interrupted`, exit 143.

## Outcome

Measure 7 days after the grh install, from transcripts with t04's script: the share of `greengate land` / `amont land` calls that hit 600 s or ran in the background without a receipt (baseline 201 of 203; target below 1 in 5). Count detached receipts ending `interrupted` or dead `running`, and any with `pushed` different from `judged`. Rollback: the rule goes off and the guard refuses `--detach` if dead or interrupted lands exceed 1 in 10, or on any `pushed` different from `judged`. Not addressed: the `git commit` chains t04 counted (t05 keeps the commit gate). Watch the `refused` receipts: a repo whose pre-push hook fast-forwards HEAD loses the detached form; if that is most refusals, add one retry on the new HEAD after the guard judges it.

## Implementation review
- implementation-review: round 1 approve-with-changes (4 blocking, 2 medium, 2 low); deltas resolved them; last delta approve-with-changes on 90bf8c50, the plan record answered here. Deliberate: the Phase 1 box waits for the release; `finish(json_out: bool)` predates this branch (only its visibility widened); the test's replace relies on compact receipt JSON.
- code-review-backend: round 1 approve-with-changes (1 high: annotated tags; 2 medium, 2 low); delta 1 medium (no push the receipt cannot record), fixed; last delta approve.
- code-review-language (rust): round 1 approve-with-changes (5 medium, 1 low); deltas resolved; last delta approve-with-changes (2 low, cosmetic).
- code-review-unix: round 1 approve-with-changes (2 medium, 3 low); resolved; last delta approve.
- After CI: `rust` (`make lint-cross`, windows target) and `windows` (test build) failed on unix-only code; `Options::rerun_flags` and the `land_detach` test module are now `#[cfg(unix)]` (92028b4a). `make lint-cross` passes locally on all three targets, and the test crate checks for x86_64-pc-windows-gnu. Delta: implementation-review approve, rust approve; backend and unix carry. Deliberate: the two bounded polls in the tests (`land_detach.rs:85`, `:319`) wait on a real child process, with no clock to inject. `load (macos-latest)` was cancelled before a runner started it (no steps ran).
- Rebased onto main after #485 (npm snapshot reuse): only CHANGELOG.md conflicted, both entries kept, this branch's entry corrected (tag object; dead worker exits 1). Push gates passed on the rebased tree; implementation-review approve on 67b3a01c. Deliberate: the died lines (`worker <pid> died`, `log:`, `rerun:`) go to stdout because they are `--wait`'s result, read by its caller, as the verdict line is.
- After the rebase, backend and unix found the `--wait` help text and the land.rs module doc still saying a dead worker exits 3; fixed to 1 (4b105340). All four approve tree de3d3778. Note for Phase 2: a dead worker and a refusal share exit 1, so a caller tells them apart by the `worker <pid> died` line.
- Phase 2 (greenbelay, F4b on each round): implementation-review, backend and lang:rust. Round 1 found a wait beside a land taking its place, no check that a detached land launched on the judged sha, and a cache key on the program name; fixed (`launched_on_another`, `binary_identity`), each delta approved; CI on Windows then failed a test that put a `C:\` path through the shell lexer (fixed, relative path). The quoted-wait fix (#283) had its own round: the joined `--wait=` form and a backslash in a double-quoted path, fixed, all three approve.
- Follow-up ticket: an annotated tag's object sha (greengate's receipt `judged`) against its peeled commit (greenbelay's judged sha) in `land-retry`, for foreground and detached tag lands alike; `set_cloexec` returning `io::Result`; the parser's flags as an `Invocation` enum; `finish`'s bool as an output enum; one `Meta` builder; relais.toml's pre-rename crate names (needs the person's re-grant).

## Full reviews (reference)
- backend: round 1 rework (3 blocking, 1 high, 1 medium, 1 low); round 2 approve-with-changes (2 high, 1 medium, 1 low); delta 1 high (lock leak); final approve.
- lang:rust: round 1 approve-with-changes (2 high, 3 medium); round 2 approve-with-changes (1 high, 3 medium, 1 low); delta 1 high; final approve.
- tui: round 1 approve-with-changes (3 high, 1 medium); round 2 approve-with-changes (3 medium, 1 low); delta 1 high; final approve.
- unix: round 1 approve-with-changes (1 blocking, 1 high, 2 medium); round 2 approve-with-changes (1 high, 2 medium); delta 1 high; final approve.
- architect: first read approve-with-changes (1 high, 1 medium, 1 low); delta 1 high (`--wait` alone); final approve.
- product owner: first read approve-with-changes (1 high, 2 medium, 1 low); final approve.
- Carried into implementation: rerun keeps the original flags; `--wait` start line; second-launch message names the running receipt; dead worker exits 3 and `--wait` re-reads before declaring death; signal handler stores the signal only (`posix_signal`); pruning removes `.log`, `.lock`, `.json.tmp`; one shared `flock` wrapper; receipt compatibility rule in the Decision log; `amont land` corpus cases; a live check that the turn is freed.
<!-- panel: repos=amont,amont-agent adds=contract reviewers=backend,lang:rust,tui,unix,architect,po body-sha=f97a50c20d50 -->
