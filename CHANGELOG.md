# Changelog

Hand-authored narrative of what shipped, per version — the human-readable half of
provenance (the attestation is the cryptographic half; see [`PROVENANCE.md`](./PROVENANCE.md)).
Newest first. The attested head is in [`LATEST.md`](./LATEST.md); each tagged
release also carries auto-generated GitHub release notes.

## v0.0.4 — 2026-06-27

- **Changed:** link-body metadata parsing (`ontology::LinkMeta`) and the
  metadata-dependent invariants — **I3** (`SHIMS_FOR` carries `notify:`), **I4**
  (`SHIMS_FOR` carries `retire_when:`), **I8** (no link metadata references a
  gitignored `_WIP/` path), plus an **I6** `reason:` exemption (#3) — completing
  the invariant set at the graph level (only I7, an emit-time rule, remains for
  M2). CI made lean: dependency caching + the per-push LTO build dropped (debug
  only; LTO at tag time) + concurrency cancellation (~18 min → minutes).
- **Tests:** `fmt` / `clippy -D warnings` / 20 unit tests green; dogfood
  (`sporaxis check`) passes both example compositions.

## v0.0.3 — 2026-06-27

- **Changed:** graph invariants **I1, I2, I5, I6** enforced with a structured
  violation report (`<code> <subject> <predicate>? <object>? <reason>`, non-zero
  exit) (#3); the **`examples/ck-allinone.composition/`** reference composition —
  the ck-allinone v0.7.22 graph, 12 entities / 17 edges (#5, M1); a **dogfood CI
  gate** that runs `sporaxis check` over `examples/*.composition` on every build.
- **Tests:** `fmt` / `clippy -D warnings` / 15 unit tests green; the binary
  validates both example compositions; SLSA Build Provenance v1 verified.

## v0.0.2 — 2026-06-27

- **Changed:** the composition-directory parser — `kernels/` + `links/` →
  typed graph (#2); the release/identity provenance discipline
  ([`PROVENANCE.md`](./PROVENANCE.md), [`LATEST.md`](./LATEST.md)) with an
  attestation-gated `update-latest-md` auto-writer (the only writer of
  `LATEST.md`); a version-less release asset
  (`sporaxis-x86_64-unknown-linux-gnu.tar.gz`) so `releases/latest/download/…` is
  a stable `curl` URL; an open-source README (badges, install, the model, I1–I8).
- **Tests:** `fmt` / `clippy -D warnings` / `cargo test` green; `gh attestation
  verify` exit 0 — `LATEST.md` bootstrapped through the gate.

## v0.0.1 — 2026-06-27

- **Changed:** scaffold — the CLI, the closed-set ontology types (5 entities / 6
  predicates), the oxigraph-backed `composition.ttl` (RDF) emitter, and a
  version-gated, SLSA-attested release pipeline on `v*` tags.
- **Tests:** `fmt` / `clippy -D warnings` / `cargo test` green.
