# Full reviews: greengate-fleet drops the aval names

## Round 1 (body 99316d1c77c4)
- **backend** (opus, 30k tok, 40 s): approve-with-changes. 1 [medium] release race with the host-slot-admission session; 2 [low] check `greengate-fleet --version`; 3 [low] census expected list; 4 [low] Phase 3 check; 5 [low] cite ADR-0073. All accepted.
- **lang:rust** (opus, 38k, 50 s): approve-with-changes. 1 [medium] delete the `rename` attributes and "Wire name kept" comments; 2 [medium] whole-text `aval` assertion on scan, Repair and Stale plans; 3 [medium] name help line, comment, `hook_flag`; 4 [low] Goal wording, four version literals. All accepted.
- **tui** (opus, 29k, 30 s): approve-with-changes. 1 [medium] refuse the old flag naming `--greentopo-hook`; 2 [medium] skip the usage dump; 3 [low] assert exit, stdout, stderr. Accepted in round 1; 1–2 later reversed by Codex's high.
- **unix** (opus, 26k, 28 s): approve-with-changes. 1 [medium] pin exit 2 and empty stdout; 2 [medium] census on HEAD then origin/main; 3 [low] the l.522 doc sentence. All accepted.

## Round 2 (body b5518d810d79)
- **tui** (33k, 14 s): approve; 1 low (binary-level test) folded in.
- **backend** (36k, 22 s): approve-with-changes; 6 [medium] the refusal cannot skip USAGE through `Err(String)`.
- **unix** (31k, 18 s): approve-with-changes; 4 [medium] same mechanism gap; 5 [low] dashboard refusal sentence in the census (carried).
- **lang:rust** (45k, 24 s): approve-with-changes; 5 [high] typed `ParseError` needed. Applied; backend re-ran (round 3): approve, 7 [low] `From` impls (carried, then moot).

## Codex (gpt, high effort)
- approve-with-changes. 1 [high] remove: a bespoke refusal keeps a live handler for the old name against "the end state holds no old name"; the unknown-argument path already refuses it. Accepted: arm deleted, no `ParseError`. 2 [medium] census classification of vendored pack and programme docs (carried).
- backend re-ran on the final body (07cb1db1ce32): approve; 8 [low] remove the no-mutation fakes, assert at the `parse` level (carried).
