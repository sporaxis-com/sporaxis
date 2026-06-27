//! Output emitters (SPEC.SPORAXIS §4 + v0.2 §C).
//!
//! One ontology → either the **dockerfile** mode (a multi-stage build) or the
//! **manifest** mode (an OCI image manifest that *references* pre-placed
//! component layers by digest — no build, no `tar -xzf` re-pack, fleet-wide layer
//! dedup) — plus `bundle.yaml`, the s6 service tree, the smoke script, the
//! CHECKLIST, and `composition.ttl` (the BOM as RDF; SPEC v0.2 §E).

use std::path::Path;

use crate::ontology::{Composition, EntityType};

/// Emit a bundle's physical outputs in the chosen mode.
///
/// v0.0.1 scaffold: resolves the mode and reports the plan. The emitters land at
/// milestone **M2**, gated on a byte-identical `diff -r` against oci-germination's
/// hand-written ck-allinone outputs.
pub fn emit(comp: &Composition, mode: &str, dir: &Path) -> anyhow::Result<()> {
    let chosen = resolve_mode(comp, mode);
    println!(
        "compose: {} entities, {} links → mode={} (from {})",
        comp.entities.len(),
        comp.links.len(),
        chosen,
        dir.display(),
    );
    // composition.ttl is emitted in BOTH modes — the BOM as RDF (SPEC v0.2 §E),
    // built on oxigraph. Wired now; the Dockerfile/manifest emitters land at M2.
    let ttl = crate::ttl::to_turtle(comp)?;
    println!("  composition.ttl: {} bytes (oxigraph Turtle)", ttl.len());
    // TODO(M2): emit, deterministically (SPEC §4 determinism rule):
    //   dockerfile mode → Dockerfile (INHERITS_FROM/COPIES_FROM/BUILDS),
    //                     s6-services/ (SUPERVISES), bundle.yaml, smoke, CHECKLIST
    //   manifest mode   → OCI config + manifest (placement_layer refs), multi-arch
    //                     index, bundle.yaml (= the layer BOM), smoke
    //   both            → composition.ttl (the BOM as RDF; SPEC v0.2 §E)
    Ok(())
}

/// `auto` selects **manifest** when every consumed entity is layer-referenceable
/// (carries a `placement_layer`, or is itself the FleetImage / a Process whose
/// runtime is supplied by a placed layer), else **dockerfile** (SPEC v0.2 §C).
fn resolve_mode<'a>(comp: &Composition, mode: &'a str) -> &'a str {
    if mode != "auto" {
        return mode;
    }
    let all_placed = comp.entities.iter().all(|e| {
        e.placement_layer.is_some()
            || matches!(e.entity_type, EntityType::FleetImage | EntityType::Process)
    });
    if all_placed {
        "manifest"
    } else {
        "dockerfile"
    }
}
