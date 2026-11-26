+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# A land refuses evidence-missing right after its own pre-stage stamped the tree

## Question

Twice on 2026-11-26, on greengate 5.2.0, `land --detach` printed
"cargo-test is declared at commit time, but N pushed commits carry no record
of it — running it here", ran it in pre-stage, printed "✓ stamped
pre-push-cargo-test … for this tree" and "✓ rehearsal of <sha> passed", and
then refused with `evidence-missing` for `pre-push-cargo-test`. A second
`land` of the same tree passed at once. Why does the pre-stage's own stamp
not count as its evidence on the first land, and should land read the stamp
it just wrote?

## Look at

- greenbelay `chore/release-6.18.1` at `988c4747`, and greengate
  `chore/t21-rename` at `a84488f8` (a merge commit: two pushed commits with no
  commit-time record). The land logs are under each worktree's
  `greengate/land/`, gone with the worktrees; the excerpts are in decisions
  topo-spotter-rename t21's Resolution, finding 1.
- The inbox line of 2026-11-25 (greenrack, `pre-push-registry` on the
  cluster executor) refuses `evidence-missing` the same way, for another
  cause.

## Answer looks like

A merged change and a test: a land whose pre-stage runs a commit-time gate
for unrecorded commits and passes it pushes on its first attempt.
