+++
type = "grilling"
mode = "hitl"
status = "open"
blocked_by = ["t02-no-timeout-in-a-moving-line"]
repos = ["amont"]
+++
# What a check does when its wait ends

## Question

Under `onBusy wait` (ADR-0009) a check whose wait times out runs anyway, outside
the queue: 29 such runs since 2026-11-21 18:00, some at load above 400, which
adds load to a host that is already jammed. The person sometimes wanted exactly
that (no wait). Once t02 decides when a wait ends, what should happen then: run
unqueued as now, defer to CI, or run unqueued only below a load ceiling? And is
"do not wait for this one" a choice the person or agent makes up front instead?
