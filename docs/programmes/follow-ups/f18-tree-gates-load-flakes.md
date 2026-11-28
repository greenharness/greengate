+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# `tree_gates` tests fail under load and pass alone

## Question

Two `tree_gates` tests failed in a 1,059 s run under load and passed when run
alone. The agent that fixed #546 saw the likely load flakes. Which two tests
are they, and what makes them depend on load?

## Look at

- The 1,059 s run's log. The failing test names were not recorded, so search
  the local and CI logs for `tree_gates` failures from 2026-11-28.
- The `tree_gates` test module: waits or wall-clock bounds that load can exceed.
- The #546 pull request, which describes the load flakes it saw.

## Answer looks like

The two test names, the wait each one depends on, and either a merged fix
after which they pass 50 runs in a row under load, or the race named.
