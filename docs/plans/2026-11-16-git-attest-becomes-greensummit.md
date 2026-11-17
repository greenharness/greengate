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

## Next in this slice

Phase 2 continues after this PR merges: a release dry run of
`release.yaml` on the first main commit CI re-signed under greensummit, then
the greengate tag, then `brew upgrade greengate` together with greenbelay's
reader release and the attest-signer image bump (Phase 3), in one step.

## Verification (observed before the push)

- Suite: `cargo test --workspace --all-features --locked --no-fail-fast` →
  every test green except two, both fixed, then rerun green:
  `names::old_data_names_are_read_only_by_the_migration` (pin counts) and
  `attest::tests::an_attestation_push_writes_only_greensummit_refs`.
  `test t ... FAILED` lines come from fixture crates that a test runs on purpose.
  The commit gate of bc375a1d ran all 29 checks green. Clippy `-D warnings`
  is clean.
- Pilot (worktree `target/debug/greengate`, scratch HOME/XDG/global git
  config, `init` not `install`; script in
  `~/.claude/greenbelay/attestations/de79ca8a/pilot.sh`):
  - **Signing at push.** Input: a pre-push gate, `greengate.attest true`,
    and a bare origin. Expected: greensummit notes. Actual:
    `refs/notes/greensummit` and `-inputs` on origin, the header
    `greensummit-v2`, and the published greensummit 2.0.0 `covered` prints
    "verified under greensummit" and the 5 gates.
  - **Old refs.** Input: the same signed notes moved under
    `refs/notes/greengate-attest(-inputs)`, on origin and locally. Expected:
    nothing covered. Actual: nothing covered, and the stale mirror is deleted.
  - **The renamed spec file.** Input: `attest check-inputs`. Expected: it
    reads `.github/greensummit-inputs`. Actual: the new path is read, and
    with the old path it reports "no .forgejo/greensummit-inputs or
    .github/greensummit-inputs".
- Release verifier: the sha256 `bc0ea42f…515eb` of
  `greensummit-2.0.0-x86_64-unknown-linux-musl.tar.gz` was downloaded and
  equals the release's SHA256SUMS line. `v2.0.0^{commit}` = `9a54c1db…74ab`,
  which CI pins.

## Decision log

- 2026-11-17 — Kept old names, as the plan's allowlist allows:
  - the signing key path `~/.ssh/greengate-attest`, with the `amont-attest`
    fallback (greengate's key, not a wire name);
  - the 3a-3 migration code and its messages (it reads the old names as
    sources);
  - the `make attest-inputs` target and the `attest check-inputs`
    subcommand (role words; only the spec path was renamed);
  - `PRE_SUMMIT_HEADERS`, recognised only to name a stale signer in a
    refusal;
  - test-only copies of the old names, which prove they are ignored.
- 2026-11-17 — `gate_stamp::READ` stays a one-element array until the
  rename's Phase 5 census, the same follow-up as greensummit's verify.rs.
- 2026-11-17 — greengate's `check-inputs` ok line moved to stderr as
  `check-greensummit-inputs: ok`, matching greensummit 2.0's library
  messages (CHANGELOG).
- 2026-11-17 — The fleet scan (`greengate-fleet` attest) matches only
  `greenharness/greensummit` pins. Repos not yet swept show no pin until
  their Phase 4 PR: accepted, since the dashboard is transient.

## Implementation review

- implementation-review (opus): approve-with-changes → approve → approve.
  Fixed: the fleet pin test expected `v1`, stale `v1` comments, a banner
  comment, the constant order. 62k+47k+27k tokens, 36+47+13 s.
- backend: approve-with-changes → approve-with-changes → approve. Fixed: a
  `StaleSigner` refusal names the stale signer, the impossible-namespace
  test, the orphan refs in the CHANGELOG, tests for both headers and the
  hint. Deliberate: `INPUTS_FULL_REF` shares `NOTES_FULL_REF`'s doc
  comment, as before. 97k+34k+24k tokens, 63+27+9 s.
- lang:rust: approve-with-changes → approve-with-changes → approve. Fixed:
  stale dual-read comments, the test name, `READ`'s deadline, one spelling of
  the namespace, the shared header list. 72k+38k+23k tokens, 54+24+8 s.
- platform: approve-with-changes → approve. Fixed: the MSRV comment (`^2`),
  the `allowed_signers` key comment. 53k+23k tokens, 33+20 s.
