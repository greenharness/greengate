+++
type = "task"
mode = "afk"
status = "open"
blocked_by = ["t06-who-may-reorder"]
repos = ["amont", "relais"]
+++
# Build: an agent can move a job ahead

## Question

Build t05's mechanism under t06's rules. First the waiter finds its ticket by
its `-{pid}-{seq}` suffix. Then come the `queue` array in `greengate slots
--json`, and `greengate slot promote <id> --reason …` with the limits t06 sets,
journalled in `waits.jsonl`.

## Look at

- t05's and t06's Resolutions.
- `crates/greenharness-slots/src/lib.rs` (`Waiter`, `live_tickets`,
  `lock_within`), `crates/greengate-runtime/src/slot_cmd.rs`.

## Answer looks like

A merged plan and release. A test moves a waiter from position 3 to position 0
while three waiters poll, and every waiter reads its true position. A promotion
past t06's limits is refused with the limit named.
