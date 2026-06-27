# sporaxis

[![CI](https://github.com/sporaxis-com/sporaxis/actions/workflows/ci.yml/badge.svg)](https://github.com/sporaxis-com/sporaxis/actions/workflows/ci.yml)
[![Release](https://github.com/sporaxis-com/sporaxis/actions/workflows/release.yml/badge.svg)](https://github.com/sporaxis-com/sporaxis/actions/workflows/release.yml)
[![Latest release](https://img.shields.io/github/v/release/sporaxis-com/sporaxis?sort=semver&display_name=tag&label=release)](https://github.com/sporaxis-com/sporaxis/releases/latest)
[![Provenance: SLSA Build v1](https://img.shields.io/badge/provenance-SLSA%20Build%20v1-2ea44f)](PROVENANCE.md)
[![License: MIT](https://img.shields.io/github/license/sporaxis-com/sporaxis?color=blue)](LICENSE)
[![Built with Rust](https://img.shields.io/badge/built%20with-Rust-orange?logo=rust&logoColor=white)](Cargo.toml)

**Ontology-first OCI bundle assembler.** Declare *what a container image is made
of* as a tiny typed graph — five entity types, six relationships — and `sporaxis`
emits everything physical: a `Dockerfile` **or** a referenced OCI manifest,
`bundle.yaml`, s6 service trees, smoke scripts, a CHECKLIST, and
`composition.ttl` (the bill of materials as RDF). The declaration is the single
source of truth; the discipline rules become **graph invariants** the assembler
refuses to violate, not paragraphs a reviewer has to remember.

```text
  one declaration                          everything physical
  ──────────────                           ───────────────────
  kernels/*.yaml      ┌───────────┐        Dockerfile  | OCI manifest
  links/*.yaml   ───► │  sporaxis │ ─────► bundle.yaml  s6 tree  smoke
  (a typed graph)     └───────────┘        CHECKLIST    composition.ttl (RDF)
                        I1–I8 ✓
```

---

## What a Dockerfile can't do

A Dockerfile is a *build recipe*. Every `FROM` / `COPY` / `RUN` produces new
layers, so the same component baked into ten images is ten separate copies on
disk. And it can't express *intent* — "never recompile an upstream," "every
service must ship a smoke test," "every patch must carry a tracked deprecation"
all live in a reviewer's head, not the file.

`sporaxis` treats a bundle as a **typed graph** instead of a script, which
unlocks things a Dockerfile structurally cannot:

- **Build-free assembly by digest (`manifest` mode).** Instead of building, it
  assembles an OCI image manifest that *references pre-placed component layers by
  their digest* — no `docker build`, no `tar -xzf` re-pack. Ten fleet images that
  all contain `pgrdf` reference the **same blob**, deduplicated fleet-wide. There
  is no Dockerfile instruction for "this image **is** these exact existing
  layers."
- **Discipline as executable invariants (I1–I8).** The assembler *refuses* to
  emit a bundle that compiles an upstream image (I1), patches one (I2), ships a
  shim without a NOTIFY + a retire-when probe (I3/I4), or runs a service with no
  smoke test (I5). A Dockerfile has no way to forbid any of that.
- **One declaration → every output.** Dockerfile *or* manifest, **plus**
  `bundle.yaml`, the s6 service tree, the smoke script, and a CHECKLIST — all
  derived from one graph, so the five places a composition used to live by hand
  can no longer drift apart.
- **The graph is a first-class RDF artifact** (`composition.ttl`): a queryable
  bill of materials — loadable into [pgRDF](https://github.com/styk-tv/pgRDF),
  governable by [pgCK](https://github.com/styk-tv/pgCK), validatable by SHACL. A
  Dockerfile leaves no machine-readable record of what's inside, or why.

## Install

**Linux x86_64** — grab the attested release binary:

```sh
curl -fsSL https://github.com/sporaxis-com/sporaxis/releases/latest/download/sporaxis-x86_64-unknown-linux-gnu.tar.gz | tar -xz
chmod +x sporaxis
sudo mv sporaxis /usr/local/bin/       # optional — put it on PATH
sporaxis --version
```

Every release tarball ships a verifiable **SLSA Build Provenance v1**
attestation. Check the supply chain before you trust the binary:

```sh
gh attestation verify sporaxis-x86_64-unknown-linux-gnu.tar.gz --repo sporaxis-com/sporaxis
```

Or build from source — any platform with a Rust toolchain:

```sh
cargo install --git https://github.com/sporaxis-com/sporaxis
```

> Today's release matrix is `x86_64-unknown-linux-gnu`; more targets are tracked
> on the [project board](https://github.com/sporaxis-com/sporaxis). See
> [`PROVENANCE.md`](PROVENANCE.md) for the full release policy and
> [`LATEST.md`](LATEST.md) for the current attested head.

## Quickstart

A *composition* is just a directory of tiny YAML files — one per entity, one per
relationship. No build script. Relationships are encoded **in the filename**:
`<subject>.<PREDICATE>.<object>.yaml`.

```sh
mkdir -p demo.composition/kernels/{pg-base,pgrdf,ck-allinone} demo.composition/links

cat > demo.composition/kernels/pg-base/kernel.yaml <<'EOF'
type: "oci:FleetImage"
version: v0.1.15
EOF

cat > demo.composition/kernels/pgrdf/kernel.yaml <<'EOF'
type: "ext:DBExtension"
version: "0.6.17"
placement_layer: "ghcr.io/styk-tv/pgrdf-bundle:0.6.17-pg17"   # a pre-placed layer → no build
EOF

cat > demo.composition/kernels/ck-allinone/kernel.yaml <<'EOF'
type: "oci:FleetImage"
version: v0.7.22
EOF

: > demo.composition/links/ck-allinone.INHERITS_FROM.pg-base.yaml
: > demo.composition/links/ck-allinone.COPIES_FROM.pgrdf.yaml
```

Validate the graph, then assemble:

```sh
sporaxis check demo.composition
# ok: 3 entities, 2 links — invariants I1–I8 pass

sporaxis compose demo.composition --mode auto
# compose: 3 entities, 2 links → mode=manifest (from demo.composition)
#   composition.ttl: … bytes → demo.composition/composition.ttl
```

`auto` picked **manifest** because every consumed entity is layer-referenceable
(each carries a `placement_layer`, or is itself a fleet image) — so the image is
assembled from existing layers with **no `docker build`**. Drop the
`placement_layer` and `auto` falls back to `dockerfile`, telling you what forced
it.

The emitted `composition.ttl` is the same graph expressed as RDF — the
ontology-first result:

```turtle
<…/compose#ck-allinone> a <…/compose#FleetImage> ;
    <…/compose#version> "v0.7.22" ;
    <…/compose#INHERITS_FROM> <…/compose#pg-base> ;
    <…/compose#COPIES_FROM>   <…/compose#pgrdf> .
<…/compose#pgrdf> a <…/compose#DBExtension> ;
    <…/compose#version> "0.6.17" ;
    <…/compose#placementLayer> "ghcr.io/styk-tv/pgrdf-bundle:0.6.17-pg17" .
```

A ready-made copy lives in [`examples/hello.composition/`](examples/hello.composition).

## The model (closed sets)

New shapes require a spec bump, not improvisation — the `enum`s in
[`src/ontology.rs`](src/ontology.rs) are exhaustive on purpose.

**Entities (5):**

| Type | Meaning |
|------|---------|
| `oci:UpstreamImage` | an image we consume but never publish (e.g. `postgres:17-bookworm`) |
| `oci:FleetImage` | an image we publish — owns a version, a tag, a manifest |
| `bin:StaticArtifact` | a binary / library / static asset, built in-tree or copied in |
| `ext:DBExtension` | a DB extension: `.so` + `.control` + optional `.sql` |
| `svc:Process` | a supervised runtime process (`longrun` \| `oneshot`) |

**Predicates (6):** `INHERITS_FROM` · `COPIES_FROM` · `BUILDS` · `SUPERVISES` ·
`SHIMS_FOR` · `SMOKES_BY`

## Output modes

- **dockerfile** — render a multi-stage Dockerfile (a build). The conventional path.
- **manifest** — assemble an OCI image manifest that *references* pre-placed
  component layers by digest: no build, no re-pack, fleet-wide layer dedup.
  Available when every consumed entity carries a `placement_layer`.
- **auto** (default) — `manifest` when every entity qualifies, else `dockerfile`,
  logging what forced the fallback.

## Invariants (I1–I8)

Discipline that a Dockerfile can't encode, enforced as graph constraints
([`src/invariants.rs`](src/invariants.rs)):

| | Rule |
|--|------|
| I1 | no `oci:UpstreamImage` is the subject of `BUILDS` — never compile upstream |
| I2 | no `oci:UpstreamImage` is the subject of `SHIMS_FOR` — never patch upstream |
| I3 | every `SHIMS_FOR` carries a `notify:` that resolves to a real NOTIFY |
| I4 | every `SHIMS_FOR` carries a `retire_when:` probe + behaviour |
| I5 | every `svc:Process` has a `SMOKES_BY` (or is explicitly `pure`) |
| I6 | duplicate-pull warning when a `COPIES_FROM` base is already inherited |
| I7 | manifest labels derive only from the `oci:FleetImage` entity |
| I8 | no committed output references a `.gitignore`'d path |

## How it fits the fleet

[`oci-germination`](https://github.com/sporaxis-com/oci-germination) is the
orchestrator and **master of bundle materialisation**; `sporaxis` is the engine
it drives — declaration → artifact. Built in **Rust** to unify the fleet toolset
(pgRDF and pgCK are Rust/pgrx).

## Status

**Scaffold (pre-M2).** The CLI, the closed-set ontology types, the directory
parser, the `composition.ttl` (RDF) emitter, and the graph invariants **I1, I2,
I5, I6** run today — the bundled `examples/ck-allinone.composition/` validates
with `sporaxis check`. The remaining invariants (I3/I4/I7/I8) and the
Dockerfile/manifest emitters land at milestone **M2**: reproduce
`oci-germination`'s `ck-allinone` bundle byte-for-byte (`diff -r`), so the
assembler is provably faithful before anything depends on it. Follow progress on
the [project board](https://github.com/sporaxis-com/sporaxis).

## Releases & provenance

`sporaxis` ships one artifact: a self-contained CLI binary, published as a GitHub
**Release tarball** with an SLSA Build Provenance v1 attestation — built and
attested exclusively by CI on a `v*` tag push (no workstation publishes).

- [`LATEST.md`](LATEST.md) — the latest **attested** release (or the honest
  "no attested release yet" bootstrap state).
- [`PROVENANCE.md`](PROVENANCE.md) — the binding release policy: build-on-CI-only,
  the attestation gate, local verification, and the coordination discipline.

Coordination is public-first: defects and tasks are GitHub **issues**; plans and
milestones live on the [project board](https://github.com/sporaxis-com/sporaxis).

## License

MIT — see [LICENSE](LICENSE).
