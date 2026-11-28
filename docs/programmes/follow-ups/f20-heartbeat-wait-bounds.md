+++
type = "task"
mode = "afk"
status = "open"
blocked_by = []
repos = ["amont"]
+++
# The heartbeat live row overstates its wait bound

## Question

The live row of the queued heartbeat says "at most 59m00s", which overstates
the bound for 11 of 35 waits. Each check has its own bound: 60 s for
`slotWait` and 600 s for `warnSlotWait`. Should the live row name each check's
own bound?

## Look at

- The live row in the t20 heartbeat plan, and its wait-bound text.
- The code that sets `slotWait` and `warnSlotWait` for each check.

## Answer looks like

The live row names each check's own bound, and the bounds it names match the
code for all 35 waits.
