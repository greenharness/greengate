---
status: active
branch: chore/t28-rename-identifiers
repos: [amont]
adrs: [ADR-0072, ADR-0073, ADR-0012]
plan-id: p-01M8FW72RZF0F82WR9FPSXFX26
---
# greengate-fleet drops the aval names

## Review panel
👉 **Decide:** none — approve if a major that drops `--aval-hook` and renames five JSON keys, with no alias, is the slice.
📍 amont · 4 phases · 1 release · next: Phase 1. Panel: backend, lang:rust, tui, unix (opus), codex.
**Changed by review:** old flag takes the plain unknown-argument path (Codex over tui's named refusal); whole-JSON `aval` assertion; release race check.
**Verdicts:** 4 approve-with-changes; round 2 tui approve, 3 approve-with-changes; backend approve on the final body; 5 carried; words 506 → 790 (×1.56, mostly checks); remove 1, merge 0.
📄 Full reviews: [2026-11-28-greengate-fleet-drops-aval-names.reviews.md](2026-11-28-greengate-fleet-drops-aval-names.reviews.md)

## Picture

```
 greengate-fleet     5.x                       6.0.0
 ---------------     ---                       -----
 fix --aval-hook     warning, works      --->  unknown argument
 --json keys         aval_hook           --->  greentopo_hook
                     no_aval             --->  no_greentopo
                     aval_hook_ignored   --->  greentopo_hook_ignored
                     aval_hook_unjudged  --->  greentopo_hook_unjudged
                     install_aval_hook   --->  install_greentopo_hook
```

The flag and the five keys t21 kept until a major take greentopo's name.

## Goal
decisions topo-spotter-rename t28, greengate slice. `greengate-fleet` stops
accepting `--aval-hook` and its `--json` output names the session-hook keys
after greentopo. t21 kept both "until the next major", so this ships as
greengate 6.0.0 (the workspace is at 5.5.0), released and installed in
Phase 4.

## Non-goals
- History comments: `crates/greengate-runtime/src/hooks/manifest_trust.rs`
  (an `aval check` of the past), `crates/greengate/tests/it/busy_defers.rs`,
  `greentopo_hook.rs`'s "aval became greentopo" doc line.
- `adr/HEADS.md` (generated ADR text), CHANGELOG history, `docs/plans`.
- Any other greengate behaviour; the major is for these names only.

## Code review
- amont: backend, lang:rust, unix, tui

## Behaviour
- Callers checked first (2026-11-28): no fleet repository, chezmoi source,
  `~/.claude` skill, hook or setting, and no grh or relais code passes
  `--aval-hook` or reads these JSON keys; the dashboard is greengate-fleet's
  own TUI, which reads the Rust types, not the JSON. greengate-fleet keeps
  no scan cache on disk to read back.
- `greengate-fleet fix --aval-hook` follows the existing unknown-argument
  path: exit 2, nothing on stdout, and the ordinary error and usage on
  stderr. There is no production branch or diagnostic literal recognizing
  the retired spelling. `--greentopo-hook` is unchanged; `--help` no longer
  names the old flag. Historical rejection-test inputs may retain it.
- `greengate-fleet scan --json` / `fix --json` keys, old → new:
  `aval_hook` → `greentopo_hook`; state `no_aval` → `no_greentopo`;
  finding kinds `aval_hook_ignored` → `greentopo_hook_ignored`,
  `aval_hook_unjudged` → `greentopo_hook_unjudged`; `install_aval_hook` →
  `install_greentopo_hook`. No alias: the old keys are neither written nor
  read.
- `docs/fleet-dashboard.md` names only the current flag and keys, dropping
  "until the next major" and the "its old name `--aval-hook` is still
  accepted" sentence (l.522). The removed spelling remains documented in
  the release notes.

## Phases
- [x] Phase 1 — rename the serde keys and their tests: delete each
  `#[serde(rename = ...)]` (`rename_all = "snake_case"` already gives the
  new names) and each "Wire name kept" comment; rename
  `the_json_keys_keep_the_old_hook_names` to say the keys follow greentopo:
  `crates/greengate-fleet/src/fix.rs`, `crates/greengate-fleet/src/scan.rs`,
  `crates/greengate-fleet/src/greentopo_hook.rs`
- [x] Phase 2 — delete the `--aval-hook` arm, its help sentence
  (l.73-74), the l.239-241 comment and the `hook_flag` plumbing; use
  `"--greentopo-hook"` directly in the ignored-flags check. Keep the
  existing parser error type and main error handling. Replace the alias
  test with rejection coverage for `fix --aval-hook` and `--bogus`, and
  retain positive coverage for `--greentopo-hook`. A binary-level test
  asserts exit 2, empty stdout and the ordinary unknown-argument error
  followed by usage. Include `--apply` and `install` rejection cases,
  using markers/fake executables to assert no mutation or child execution.
  Assert usage contains no retired spelling:
  `crates/greengate-fleet/src/main.rs`
- [x] Phase 3 — docs: `docs/fleet-dashboard.md`
- [ ] Phase 4 — run `greenbelay board --repo amont`, rebase on origin/main
  and read `Unreleased` and the latest tag right before tagging; another
  session's entries there ship in 6.0.0 and are named in its notes. Then
  release 6.0.0 through the `tag-release` skill, moving
  `[workspace.package] version` and the three `[workspace.dependencies]`
  literals, the dropped
  flag and five keys named in its notes, install it, print
  `greengate --version` and `greengate-fleet --version`: `CHANGELOG.md`,
  `Cargo.toml`, `Cargo.lock`

## Decision log
- 2026-11-28 — a major, because t21 promised the JSON keys and the flag
  would keep their names until one (`cli.compat.interface-is-public`).
- 2026-11-28 — no read alias for the old keys: nothing outside the crate
  reads them, and an alias would be a live reader of `aval`.
- 2026-11-28 — 5.6.0 did not ship (perso-f7's local gate failed
  `slot_run::a_session_moves_its_own_waiting_job_to_the_head`), and they
  released this branch to go first: 6.0.0 carries #541, #542 and #544 in its
  notes. Rebased onto main 0286c855 before the last review round.
- 2026-11-28 — clippy and the tests ran on the pinned 1.94.1 with
  `PATH=$HOME/.cargo/bin:$PATH`: Homebrew `rust` 1.100.0 (reinstalled
  11-27) shadows the rustup pin and fails pre-existing code
  (`greengate-runtime` `read`/`write` symbols). Not this branch's to fix.

## Verification
- Phase 1: `cargo test -p greengate-fleet` (through `greengate slot run`) →
  the JSON tests assert the new keys and
  `!value.to_string().contains("aval")` on the scan, the Repair plan and
  the Stale plan → on 94ec2bb1 with the pinned 1.94.1: `cargo clippy -p
  greengate-fleet --all-targets -- -D warnings` clean, 198 passed (170
  unit + 28 it);
  the whole-text check became `assert_no_old_name`, a walk of keys and
  `state`/`warning` tags (review: paths may hold anything)
- Phase 1–2: piloted `greengate-fleet scan --json --root <scratch fleet>`
  on a scratch repo with an `.adr.yaml` → `greentopo_hook` present, no key
  matching `aval` → `greentopo_hook` state `stale`, no `aval` in the JSON;
  `greengate-fleet fix --aval-hook --json >out 2>err` → exit 2, `out` 0
  bytes, `err` = `greengate-fleet: unknown argument "--aval-hook"` then
  the usage (46 lines); `fix --apply --aval-hook` exit 2; `--help | grep -c
  aval` 0. The binary-level test
  `the_removed_hook_flag_is_refused_before_any_write` pins the same, plus
  `install` and `--bogus` and an untouched hooks dir
- Phase 3: `grep -nE 'aval_hook|no_aval|until the next major|still accepted'
  docs/fleet-dashboard.md` → no output (grep exit 1)
- Phase 4 (next, the release): the four version literals read 6.0.0;
  `which -a greengate-fleet`; `greengate --version` and
  `greengate-fleet --version` → 6.0.0
- Census, on `HEAD` before the push and on `origin/main` after the merge,
  outside CHANGELOG, `docs/plans` and `adr/HEADS.md`: `git grep -nIiE
  '(^|[^[:alpha:]])aval|adr-lint'` plus case-sensitive `git grep -nE
  '[a-z]Aval'` → only `greentopo_hook.rs`'s doc line, `manifest_trust.rs`,
  `busy_defers.rs`, and `--aval-hook` as rejection-test input in `main.rs` →
  on HEAD 94ec2bb1: those, plus the rejection inputs in `tests/it/flows.rs`
  and `assert_no_old_name`'s doc and filter in `fix.rs` (tests forbidding
  the old name); nothing else

## Implementation review
- implementation-review (opus): approve-with-changes ×3 rounds; fixed: binary-level rejection test, key walk not text, out-param `json_names`, `root`/`args` names; ticks and results here. ~134k tok, 112 s.
- lang:rust: approve; low (path-sensitive `aval` check) fixed. 42k, 28 s.
- backend: approve; lows fixed (path check) or recorded (no expand step for keys: census, CHANGELOG). 48k, 31 s.
- unix: approve-with-changes → approve on delta; CHANGELOG says no release warned and the census stands in. 74k, 46 s.
- tui: approve, no findings (no did-you-mean by design: no live reader of the old name). 42k, 24 s.

## Outcome
<!-- panel: repos=amont reviewers=backend,lang:rust,tui,unix,codex body-sha=07cb1db1ce32 -->
