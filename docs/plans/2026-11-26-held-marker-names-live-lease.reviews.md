# Full reviews: held marker names a live lease

Round 1 (all approve-with-changes, all findings accepted and applied by hand, because `plan-review record` found no transcript): tui 30k/29 s, backend 33k/44 s, unix 31k/36 s, po 34k/42 s, architect 46k/74 s, lang:rust 49k/82 s. Round 2 (resumed): architect approve, unix approve, po approve, tui approve, lang:rust approve, backend approve-with-changes.

Carried into implementation (round 2, none changes the body):
- backend: keep the once-per-process flag on a caller-owned `MarkerLog`, never a crate-global (parallel tests would share it); state that an absent `admission` file means no lock is tried and no file is created; count the phase-3 gate on accepted `held:<id>` markers, since rows cannot tell the hook path from `slot run`.
- lang:rust: the `marker_bare` once-per-process test lands with phase 2, not phase 3; the stderr line uses `escape_default`, the journal row the crate's JSON `escape()`, so `jq` parses it.
- tui: escape first, then cut to 64, so the cap bounds what the terminal shows.
- architect and po: Non-goals says no up-front bump of the vendored greengate, a bump only when the phase-3 gate sees a 4.x writer.


Final body: backend's round-2 findings, rust's marker_bare placement and JSON escape, tui's escape-then-cut and the Non-goals exception were then applied to the body; backend re-ran on it: approve (49k, 54 s). Still carried: greengate holds a `MarkerLog` beside its `OnceLock<Marker>` (backend); `held_marker_in` timing with 16 records (rust).
