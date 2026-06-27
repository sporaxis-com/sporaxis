# Build provenance & release policy

`sporaxis` ships exactly one artifact: a self-contained `sporaxis` CLI binary,
published as a GitHub **Release tarball** with an SLSA Build Provenance v1
attestation. There is no GHCR image — `sporaxis` is the engine
[`oci-germination`](https://github.com/sporaxis-com/oci-germination) drives, not
a bundle it assembles. This document is the binding contract for how that binary
gets built, attested, and advertised.

## Hard rules

1. **All release builds run on GitHub Actions only.** The release tarball and its
   attestation are produced exclusively by `.github/workflows/release.yml` on a
   `v*` tag push. Workstation `cargo build` + `gh release create`, `gh release
   upload`, or any local-credential publish is prohibited. A local build is for
   testing; it is never a release.
2. **`LATEST.md` MUST NOT carry any version that lacks a verifiable SLSA Build
   Provenance v1 attestation.** If `gh attestation verify` rejects (or has no
   record of) the tarball in question, that version is not "the latest" — the
   file stays where it was. There is no manual-edit exception, not even to seed
   initial state. When no attested release exists yet, `LATEST.md` says so
   plainly. (Bootstrap status is in [`LATEST.md`](./LATEST.md) itself.)
3. **A new version tag MUST NOT be pushed unless the previous tag is already
   advertised in `LATEST.md`** (once the first attested release has landed — see
   *Bootstrap*). Concretely: do not tag `v0.1.0` until `v0.0.2` shows up in
   `LATEST.md`. This guarantees the previous release went through the attestation
   gate end-to-end. Tagging ahead of the gate creates orphan releases the policy
   cannot retroactively verify.
4. **The version gate is `Cargo.toml` == tag.** `release.yml` reads
   `package.version` from `cargo metadata` and refuses to proceed unless
   `v<version>` equals the pushed tag. Bump `Cargo.toml::package.version`, commit,
   *then* tag. Never tag a commit whose `Cargo.toml` version disagrees with the
   tag — CI will reject it.
5. **Gate before publish.** `release.yml` runs `cargo clippy --all-targets -- -D
   warnings` and `cargo test` *before* it builds, packages, attests, or
   publishes anything. A red gate means no tarball, no attestation, no release —
   by construction, not by convention.
6. **Versions are monotonic and never reused.** A failed release at `vN.M.K`
   retires that number permanently; the next attempt is `vN.M.(K+1)`. Do not
   `git push origin :refs/tags/<name>` to delete a failed tag, and do not `git
   tag -f` to move one. The tag stays where the failure happened; the fix is a
   new commit with a new tag. Gaps in the version sequence are expected and are
   explained by the corresponding GitHub Actions run history.
7. **Report version + closed work every release turn.** When a tag is pushed (or
   proposed), the user-facing turn summary states the version, the issues/board
   cards it closes (e.g. `v0.1.0 — closes #2, #3`), and confirmation that the
   attestation gate passed (`gh attestation verify` exit 0). A release is not
   "done" until that confirmation exists.
8. **Pin only attested digests in emitted artifacts.** When `sporaxis` emits a
   referenced OCI manifest or `composition.ttl` that pins upstream component
   layers by digest, those digests must come from artifacts that pass `gh
   attestation verify`. An unverifiable upstream digest is not consumable; surface
   it, hold back to the last attested digest, or take an explicit, recorded user
   override. Silent consumption of an unattested upstream is prohibited — this is
   the same gate `oci-germination` applies before bumping a pin.

Everything else in this document explains how those rules are enforced.

---

## What's enforced

| Surface | Built / published by | Provenance |
|---|---|---|
| `sporaxis-x86_64-unknown-linux-gnu.tar.gz` (release tarball; version carried by the release tag) | `release` workflow on `v*` tag push | [SLSA Build Provenance v1](https://slsa.dev/spec/v1.0/provenance) via [`actions/attest-build-provenance@v2`](https://github.com/actions/attest-build-provenance) — the tarball is the attested subject |
| `SHA256SUMS` (checksum) | same workflow, same run | `sha256sum` of the tarball produced by the run that attested it |
| `https://github.com/sporaxis-com/sporaxis/releases/tag/v<ver>` | `release` workflow's final job (`softprops/action-gh-release`, auto-generated notes) | The GitHub-rendered release notes are the per-version narrative |
| `LATEST.md` at the repo root | `update-latest-md` workflow on successful `workflow_run` of `release` | Refuses to advance unless `gh attestation verify` accepts the new tarball |

If `gh attestation verify` rejects the tarball, `LATEST.md` stays where it was.
That is how a workstation push gets caught — it cannot produce a valid
GitHub-issued OIDC attestation.

**Platform coverage.** Today the release builds `x86_64-unknown-linux-gnu` only.
The multi-platform matrix (additional targets + cross-compile) is tracked as a
release task on the [project board](https://github.com/sporaxis-com/sporaxis) —
until it lands, `LATEST.md` advertises the single Linux/amd64 tarball.

## Verifying a release locally

Download the tarball and its `SHA256SUMS` from the release, then:

```sh
# checksum
sha256sum -c SHA256SUMS

# SLSA Build Provenance v1 — file-based subject (not an oci:// ref)
gh attestation verify sporaxis-x86_64-unknown-linux-gnu.tar.gz \
  --repo sporaxis-com/sporaxis
```

A successful verify means:

- Signed by GitHub's Fulcio CA against the OIDC token of a specific workflow run
- That workflow run is in `sporaxis-com/sporaxis`
- The signature is recorded in Sigstore's Rekor transparency log
- The subject digest matches the tarball you downloaded

## Cutting a release (the only allowed flow)

1. Bump `Cargo.toml::package.version` to the new `<ver>` (Rule 4).
2. Commit.
3. Tag: `git tag -a v<ver> -m "<short>"`.
4. Push the tag: `git push origin v<ver>`.

GitHub Actions takes over: gate (version == tag, clippy, test) → build → package
+ checksum → attest → publish the GitHub release. No step in this flow requires
`gh release create`, `gh release upload`, or any local-token credential. Wait for
the run to report success and confirm `gh attestation verify` exits 0 against the
published tarball before calling the release done (Rule 7).

## Bootstrap

Rule 3 ("previous tag must be in `LATEST.md`") takes effect from the **first
attested release** onward. `v0.0.1` is the bootstrap: it is the first tag pushed
after `release.yml` existed, so its workflow run is the first to issue an
attestation. Until that run completes and the tarball verifies, `LATEST.md`
stays in the "no attested release published yet" state — that is the correct,
policy-compliant bootstrap value, not a placeholder to be hand-filled.

The LATEST.md auto-writer is **wired**:
[`.github/workflows/update-latest-md.yml`](./.github/workflows/update-latest-md.yml)
fires on a successful `release` `workflow_run` (or manual dispatch with a tag),
downloads the release tarball, runs `gh attestation verify` as a hard gate,
computes the digest, and renders `LATEST.md` via
[`tools/render-latest-md.py`](./tools/render-latest-md.py) — the single allowed
writer (Rule 2/3). If the verify fails, `LATEST.md` is left untouched. The
renderer refuses to emit a version without a verified digest, so the bootstrap
state can only advance off attested content; a version is never hand-written in.
Every successful `release` run advances `LATEST.md` on its own from here on. The
`v0.0.1` bootstrap is advanced by a one-time `workflow_dispatch` (tag `v0.0.1`),
since that release shipped before this workflow landed on `main` — verified, not
hand-written, exactly as the gate requires.

---

## Public coordination & disclosure discipline

This repository — its issues, pull requests, commit messages, and committed docs
(including this file and `LATEST.md`) — is **PUBLIC**. Cross-repo coordination
runs on two channels split by sensitivity, summarised below. Confidential
material is kept off this public surface; only the disclosable summary appears here.

- **Public channel — GitHub issues / PRs / the [project board](https://github.com/sporaxis-com/sporaxis).**
  For anything safe in the open: concrete defects and discrete tasks → **issues**;
  plans, roadmaps, milestones (M1, M2, the release matrix) → the **board**, not a
  pile of speculative issues. Asks *about the assembler* — a byte-diff mismatch in
  an emitted Dockerfile, a missing predicate, a manifest-mode gap — arrive here as
  issues against this repo and are triaged here.
- **Private channel — gitignored `_WIP/` NOTIFIES.** For anything confidential:
  internal consumer codenames, security internals, embargoed upstream fixes,
  unreleased architecture, partner detail. `_WIP/` is never committed; only `_WIP/`
  may name confidential entities.

Before opening or commenting on a public issue/PR — or writing a commit message —
ask: *would this sentence be safe on the project's front page?* If not, move it to
`_WIP/` and link nothing from the public side. **Scrub before posting:** local
home/filesystem paths, internal operator/cluster names, internal consumer
codenames, `_WIP/` contents, private `SPEC*` text, secrets/credentials, and
unreleased security detail. A leak is a **confidentiality incident**, not a style
nit.

**Routing.** `oci-germination` is the fleet integrator — it drives `sporaxis` and
re-cuts the bundles. Asks *to* `sporaxis` land as issues on this repo. Asks *from*
`sporaxis` to a boundary (a `placement_layer` contract change, an upstream digest
that won't attest) go to that boundary's public surface when disclosable, or to a
`_WIP/` NOTIFY when not. Keep working the honest-degrade path; never block on a
pending ask.

## Audit trail

- Release workflow: [`.github/workflows/release.yml`](./.github/workflows/release.yml)
- CI gate (fmt / clippy / build / test): [`.github/workflows/ci.yml`](./.github/workflows/ci.yml)
- LATEST.md auto-writer: [`.github/workflows/update-latest-md.yml`](./.github/workflows/update-latest-md.yml) + [`tools/render-latest-md.py`](./tools/render-latest-md.py)
- Attestation generator: `actions/attest-build-provenance@v2` (Sigstore-backed)
- Verifier: `gh attestation verify` (built into `gh` 2.49+)
- Coordination protocol: maintained privately, outside the committed tree
- Latest attested artifact: [`LATEST.md`](./LATEST.md)
