# Follow-ups

## Destination

Work noticed while doing something else in greengate is a ticket here, and each one is closed, or knowingly dropped, by a merged change.

## Notes

- Lines filed from the repository inbox at a session boundary (worktree-task Handoff).
- One pull request per ticket, unless two tickets touch the same function.

## Decisions so far

- [f01 lock-route polish](f01-lock-route-polish.md): the production export is asked lazily, once per lock directory and only on findings; every lock-route warning names its Next step. greengate#501, unreleased.
- [f03 unchecked cache masks an upgrade](f03-unchecked-cache-masks-upgrade.md): an `unchecked` record carries the version that wrote it and answers only to that version; clean and findings records still answer across versions. Unreleased.
- [f04 lock-route names](f04-lock-route-names.md): the registry parameter stays `ctx` in all 15 checks (one name for one type); the test scratch directories are `scratch_dir`.
- [f05 rehearsal uses the live venv](f05-rehearsal-uses-live-venv.md): a push snapshot puts its own copies of the borrowed venv's editable `.pth` directories first on `PYTHONPATH`. Unreleased.
- [f06 land version split](f06-land-version-split-with-hook.md): land refuses before its pre-stage when the pre-push hook would run another greengate version, naming both and the re-bake; `greengate init` from a worktree's `node_modules` keeps re-baking the shared hooks (the repository's pinned version). Unreleased.

## Handoff

2026-11-21, boundary: greengate 4.12.0 released (PR #498, release #499), with `pre-push-audit-python` reading `uv.lock` when there is no virtualenv. It is installed on this machine and verified on a fresh sre-agent clone: 1 private package skipped, 10 known findings. sre-agent's own findings are its ticket t32, due 2026-11-27. The four tickets below came from that work. Next, in order: f03 (the cache masks an upgrade, the one users hit), then f02 (ADR-0055 on the venv route, due 2026-12-05), then f01, then f04.

2026-11-21: f01 closed first, on the relay's prompt (lazy production export, Next lines; unreleased). f03 is still the one users hit; next, in order: f03, f02, f04, then f05 (the rehearsal tests against the live venv; filed from the inbox). f01 merged as greengate#501 and is not released yet: it ships with the next release.

2026-11-21: f03 closed (unchecked records carry their writer's version; unreleased, ships with f01 in the next release). Next, in order: f02 (ADR-0055 on the venv route, due 2026-12-05), f04, then f05.

2026-11-21: f04 closed on the relay's prompt (`ctx` kept, test `tmp` renamed `scratch_dir`; test-only, nothing to release). Next, in order: f02 (ADR-0055 on the venv route, due 2026-12-05), then f05. f01 and f03 still ship with the next release.

2026-11-21: f05 closed on the relay's prompt (a push snapshot puts its own copies of the borrowed venv's editable `.pth` directories first on `PYTHONPATH`; unreleased, ships with f01 and f03 in the next release). Filed f07 (`uv run` in a `testPushedTree` snapshot runs a `PATH` pytest; numbered f06 at first, renumbered because #505 filed its own f06 meanwhile). Next, in order: f06 (land refuses at once when the hook runs another greengate), f02 (ADR-0055 on the venv route, due 2026-12-05), then f07.

2026-11-22: f06 closed on the relay's prompt (land refuses at once on a hook/land version split; `init` unchanged; unreleased, ships with f01, f03 and f05 in the next release). Next, in order: f02 (ADR-0055 on the venv route, due 2026-12-05), then f07.

2026-11-24: filed f08, f09 and f10 from the inbox. They are four tests that failed the host-slot-admission t11 lands at load 40–150 and pass alone; the branch touched none of them. Next, in order: f02 (due 2026-12-05), f07, then f09, f08 and f10. f09 comes first of the three because its fix (count waits, not seconds) is already clear.

2026-11-26: filed f13 (a land refuses `evidence-missing` right after its own pre-stage stamped the tree; seen twice on 5.2.0) and f14 (`cluster_gates::concurrent_gates_each_get_their_own_job` flaked on release PR #534), from decisions topo-spotter-rename t21. Next, in order: f02 (due 2026-12-05), f13, f07, then f09, f08, f10, f14.

## Not yet specified

## Out of scope
