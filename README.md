# sporaxis

**Ontology-first OCI bundle assembler.** A declaration goes in — a closed-set
composition ontology (five entity types, six predicates) — and everything
physical comes out: a `Dockerfile` *or* a referenced OCI manifest, `bundle.yaml`,
s6 service trees, smoke scripts, a CHECKLIST, and `composition.ttl` (the bill of
materials as RDF). The ontological output is as first-class as the image it
produces.

A gene/spore-level binary assembler for the sporaxis fleet:
[`oci-germination`](https://github.com/sporaxis-com/oci-germination) remains the
orchestrator and **master of bundle materialisation**; `sporaxis` is the engine it
drives — declaration → artifact.

## Why

Today a bundle's composition lives in five hand-edited places at once (a
multi-stage Dockerfile, an s6 service tree, smoke scripts, a CHECKLIST, and prose
rules). Cross-cutting changes leak into all five with no shared source of truth.
`sporaxis` makes **one declaration the single source** and derives the rest —
discipline rules become **graph invariants** the assembler refuses to violate
(I1–I8), not paragraphs a reviewer must remember.

## The model (closed sets)

**Entities (5):** `oci:UpstreamImage` · `oci:FleetImage` · `bin:StaticArtifact` ·
`ext:DBExtension` · `svc:Process`

**Predicates (6):** `INHERITS_FROM` · `COPIES_FROM` · `BUILDS` · `SUPERVISES` ·
`SHIMS_FOR` · `SMOKES_BY`

New shapes require a spec bump, not improvisation — the `enum`s in
[`src/ontology.rs`](src/ontology.rs) are exhaustive on purpose.

## Two output modes

- **dockerfile** — render a multi-stage Dockerfile (a build). Today's path.
- **manifest** — assemble an OCI image manifest that *references* pre-placed
  component layers by digest: no build, no `tar -xzf` re-pack, fleet-wide layer
  dedup. Available when every consumed entity carries a `placement_layer` (the
  C1–C4 contract).
- **auto** (default) — manifest when every entity qualifies, else dockerfile,
  logging what forced the fallback.

## Usage

```sh
sporaxis check   ck-allinone.composition/             # parse + invariants I1–I8
sporaxis compose ck-allinone.composition/ --mode auto # emit the physical outputs
```

## Status

**v0.0.1 — scaffold.** The CLI and the closed-set ontology types are real; the
directory parser, the I1–I8 invariants, and the emitters land at milestone
**M2**: reproduce `oci-germination`'s current `ck-allinone` bundle
byte-for-byte (`diff -r`) so the assembler is provably faithful before anything
depends on it.

Built in **Rust** to unify the fleet toolset (pgRDF and pgCK are Rust/pgrx).

## License

MIT — see [LICENSE](LICENSE).
