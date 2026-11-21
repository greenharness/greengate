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

## Handoff

2026-11-21, boundary: greengate 4.12.0 released (PR #498, release #499), with `pre-push-audit-python` reading `uv.lock` when there is no virtualenv. It is installed on this machine and verified on a fresh sre-agent clone: 1 private package skipped, 10 known findings. sre-agent's own findings are its ticket t32, due 2026-11-27. The four tickets below came from that work. Next, in order: f03 (the cache masks an upgrade, the one users hit), then f02 (ADR-0055 on the venv route, due 2026-12-05), then f01, then f04.

2026-11-21: f01 closed first, on the relay's prompt (lazy production export, Next lines; unreleased). f03 is still the one users hit; next, in order: f03, f02, f04, then f05 (the rehearsal tests against the live venv; filed from the inbox). f01 merged as greengate#501 and is not released yet: it ships with the next release.

2026-11-21: f03 closed (unchecked records carry their writer's version; unreleased, ships with f01 in the next release). Next, in order: f02 (ADR-0055 on the venv route, due 2026-12-05), f04, then f05.

2026-11-21: f04 closed on the relay's prompt (`ctx` kept, test `tmp` renamed `scratch_dir`; test-only, nothing to release). Next, in order: f02 (ADR-0055 on the venv route, due 2026-12-05), then f05. f01 and f03 still ship with the next release.

## Not yet specified

## Out of scope
