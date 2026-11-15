# Full reviews: snapshotDeps reuse for npm and yarn 1

Round 1, all approve-with-changes (opus-5-5): tui 4 findings (31k, 35 s); po 5 (29k, 40 s); unix 3 (38k, 51 s); backend 7 (38k, 62 s); lang:rust 5 (39k, 69 s); architect 4 (44k, 69 s). Round 2 (resumed): backend 3 new (47k, 16 s); po 4 (36k, 13 s); unix 2 (44k, 18 s); tui 2 (37k, 16 s); architect 2 (53k, 31 s); lang:rust 3 (53k, 40 s). Backend delta on the final body: 3 (55k, 23 s). Disk-reframe deltas (backend): 7 findings incl. one blocking (macOS `cp -c` may silently copy; now Phase 0 case (g) plus an `st_dev` refusal) (65k, 31 s); two contradictions fixed (73k, 26 s); final binding, no blocker (71k, 48 s).

Rejected: backend 4 (25% bar; po's 50% bar kept as the single number); architect 1 (one-key JSON scan; `node -e` helper chosen, left to the person).

Round 3 (all six resumed on the disk-framed body): backend 3 low (81k, 10 s); tui 3 (48k, 21 s); po 4 incl. one high (47k, 19 s); architect 2 (66k, 35 s); lang:rust 3 incl. one blocking (69k, 52 s); unix 4 (62k, 56 s). Applied: ubuntu CI reflink mount and no stub-`cp` test (rust 9, 10), unconditional `st_dev` check on `dst.parent()` (unix 3, rust 11), node fed on piped stdin with no temp file (unix 4), Phase 1 split into 1a (`clone_dir`, shippable alone) and 1b (gated plumbing) (architect 7), exact refusal and reuse-line wording (tui), 0 GB standing space stated (po 1, 3), 10% disk bar scoped to npm and yarn (backend).

👉 Decision left to the person: po's value gate (high): build Phases 2-3 only if a repo of yours uses npm or yarn. You chose to keep the plan whole, so it is not applied.

Carried into implementation, not edited into the body:
- Backend's last three: say "untouched" in 1b; "0 open blockers" wording; Phase 1a's first step is a throwaway CI run proving `mount -o loop` on the runner (if not, probe `cp --reflink=always` and skip, with one named test asserting the refusal).
- Consider `statfs` `f_fstypename == apfs` in `clone_dir` beyond `st_dev` (backend).
- Name `ci.yaml` in the release note and say `real_repo` gains a fail-not-skip mode (po).
- A kill mid-clone leaves a partial clone in the snapshot, like today's install leftover; on APFS it shares blocks (unix 5).
- A forced early `node` exit with more than 64 KiB of input must not hang greengate (unix, test to add).
- Phase 0 (g) also run on a second APFS volume: exit code and `df -k` delta (architect, unix).
