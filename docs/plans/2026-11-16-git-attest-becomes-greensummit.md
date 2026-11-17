---
canonical: greenharness/greensummit:docs/plans/2026-11-16-git-attest-becomes-greensummit.md
phases: [2]
status: active
branch: feat/greensummit
plan-id: p-01M7JZ490DTGP50C29SRAYEMYB
---
# git-attest becomes greensummit

Pointer: the canonical plan lives in greenharness/greensummit. This
repository carries Phase 2: greengate writes and reads greensummit notes
(`greensummit = "2"`), drops the LEGACY `amont-attest` read (attest notes and
gate stamps), and its release verifier downloads greensummit 2.0.0
(ADR-0073).

## Code review
- amont: backend, lang:rust, platform (unix, tui left out: no CLI change but a dependency; architect left out: attest's seat covers the format contract; lang:python: none touched)
