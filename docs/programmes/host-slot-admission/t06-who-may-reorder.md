+++
type = "grilling"
mode = "hitl"
status = "open"
blocked_by = ["t05-reorder-mechanism"]
repos = ["amont", "relais"]
+++
# Who may move a job ahead, and how far

## Question

With the mechanism from t05: who may reorder (any agent, only the job's own
session, the person), how far (to the head, past another session's job, past a
job already waiting longer than a bound), what reason must be given and where it
is journalled, and what stops every agent from always jumping (a budget per
session per day, a cap on jumps per job, an age floor that no jump passes)?
