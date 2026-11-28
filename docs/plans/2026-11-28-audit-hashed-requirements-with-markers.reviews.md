# Full reviews: Audit hashed requirements with markers

## Round 1 (body 9184e6342a14)

- backend (opus, 46.7k, 105 s): approve-with-changes. 1 medium timeout
  semantics across directories, budget scope vs Non-goals (accepted);
  2 medium unobservable Verification rows (accepted); 3 medium no timing
  numbers (accepted); 4 low unix left-out reason (accepted: unix declared).
- lang:rust (opus, 47.1k, 56 s): approve-with-changes. 1 high one output
  string for skip tables and full (accepted: `out` and `full` apart);
  2 high timeout meaning (accepted); 3 medium timeout test (accepted);
  4 medium waiver test (accepted); 5 medium assert a PYSEC id (accepted);
  6 low name the inverted assertion (accepted).
- tui (opus, 35.6k, 42 s): approve-with-changes. 1 high gap with no reason
  or next (accepted); 2 medium "NOT fully checked" (accepted); 3 medium an
  ok line (accepted).
- unix (opus, 40.6k, 42 s): approve-with-changes. 1 medium PrivateDir and
  batch-file failures silent (accepted, read-only TMPDIR e2e); 2 low budget
  bound overstated (accepted).

## Round 2 (body c800ac9aa2e7)

- lang:rust (opus, 32.7k, 22 s): approve; all six resolved.
- backend (opus, 33.6k, 39 s): approve-with-changes; 1-4 resolved. New:
  5 medium Non-goals says rust/go/js keep behaviour, but the shared core's
  timeout change reaches them; add a waiver case to the two-directory test
  (carried into implementation); 6 low Timing row has no expected value:
  expected under `greengate.timeout` (carried).
- unix (opus, 28.0k, 20 s): approve-with-changes; 1-2 resolved. New: low,
  the ok line must be the lock route's shape `audit-python: audited <path>
  (N package versions, M skipped)` (carried).
- tui (opus, 29.8k, 37 s): approve-with-changes; 1-2 resolved, 3 not
  (same ok-line shape as unix, medium, carried); low: assert the
  `TIMEOUT_NEXT` line and the ok line in the tests (carried).

## Codex (final, body c800ac9aa2e7)

- codex (high effort): approve-with-changes.
  1 high: hashes prove no registry provenance; stripping markers can send
  an off-host private name. Rejected: the requirements route has never had
  source provenance (pip-audit -r has sent every name of the file since
  before f108); this changes which pins of the same file are audited, not
  the sending policy. Filed as a follow-up.
  2 medium: "every pin skipped" by distinct names miscounts a name with one
  version audited and one skipped (carried into implementation).
  3 medium: assert every batch's input reached pip-audit, and a case whose
  only finding is in the last batch (carried).
  4 low: JavaScript does not call the wrapper; only rust and go do (carried).
