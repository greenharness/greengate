# Reviews: The slot journal records holders and hold time


Carried into implementation (applied while building, not in the body):
- Goal wording: "every lease dropped normally" (backend r2#7).
- `jq` pairing line sorted by `at_ms`; Phase 3 quotes the full `jq -cR 'fromjson? | select(has("waited_ms"))'` line (backend r2#8, tui r3#4, architect r2#3).
- "never equal" becomes "equal at best" (rust r2#1).
- Pilot tolerance widened to +1000 ms; the pilot forces 2 slots (`GREENGATE_HOST_SLOTS=2`), clears `GREENHARNESS_HOST_SLOT`, uses the worktree binary and starts the second run after the first's admitted row (rust r2#2, tui r2#4, codex#3).
- The interrupted-holder note also goes in host-slots.md (tui r2#5).
- In-flight leases are not counted as unpaired (pid dead or older than the longest hold) (unix r3#2).
- A measurable stale-writer check: post-install greengate/relais admitted rows without `holders` = 0 (architect r2#4, r5).
- The rollback baseline: the pre-4.13 rows carry no `holders`, so compare all admitted rows before and after, and the contended rows from the 4.13 rows (tui r5#1, rust r5#1, po r5).
- The scan is approximate: a detected read failure writes null, never a 0 count (codex#2).
- A failed grant write is ignored, as today (architect r5).
- `raw(s, "lease")`: the key goes before `command` (backend r5).

Round 1 (backend, rust, tui, unix: all approve-with-changes, 26–41k tokens, 20–60 s):
- tui: jq readers, one-liners, rotation non-goal.
- backend: adds=contract (blocking), pairing (high), relais code review (high), test syntax, tolerance, growth.
- unix: monotonic clock (high), pairing tolerance, signal exits skip Drop, line order.
- rust: Instant not probe clock (high), pairing, Debug, waits() helper.

Round 2 (all approve or approve-with-changes, 31–52k):
- architect r1: older-writer rows (high), the has("waited_ms") idiom.
- po r1: a Measure section (high), censoring, Phase 4 separable, the Resolution narrowing, the growth figure.
- unix r2: jq -R fromjson?.

Round 3 (delta): all approve or approve-with-changes; lows on `held_ms ≥ 0`, the jq form and the measure wording.

Codex (gpt, high effort): approve-with-changes. #1 record equality is not lease identity (high, applied). #2 the scan is approximate (medium, carried). #3 pilot concurrency (medium, carried).

Round 4 (delta on the codex edit): backend high (the id lives on Lease only, semver), architect medium (same), rust/tui/unix lows (private writer, `raw`, `slots --json` unchanged, a temp file and rename at the grant), po A/B (a numeric rollback, a `waited_ms` test). All applied.

Round 5 (final delta): backend, rust, unix, architect and po approve; tui approve-with-changes (low, carried). 9–16 s each.

