//! The closed-set composition ontology (SPEC.SPORAXIS §2–§3).
//!
//! Five entity types, six predicates. New shapes require a spec bump, not a
//! vocabulary extension — so these are exhaustive `enum`s on purpose. This is the
//! "ontology" in ontology-first: the declaration is typed against these, and the
//! same graph renders to a Dockerfile, an OCI manifest, or `composition.ttl`.

use std::path::Path;

use serde::Deserialize;

/// SPEC §2 — the closed set of entity types. Five is normative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum EntityType {
    /// A container image we consume but do not publish (e.g. `postgres:17-bookworm`).
    #[serde(rename = "oci:UpstreamImage")]
    UpstreamImage,
    /// A container image we publish; owns a version, a GHCR tag, a manifest.
    #[serde(rename = "oci:FleetImage")]
    FleetImage,
    /// A binary/library/static asset — built in-tree or copied from an upstream.
    #[serde(rename = "bin:StaticArtifact")]
    StaticArtifact,
    /// A DB extension delivered as `.so` + `.control` + optional `.sql`.
    #[serde(rename = "ext:DBExtension")]
    DbExtension,
    /// A supervised runtime process (`longrun` | `oneshot`).
    #[serde(rename = "svc:Process")]
    Process,
}

/// SPEC §3 — the closed set of predicates. Six is normative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum Predicate {
    #[serde(rename = "INHERITS_FROM")]
    InheritsFrom,
    #[serde(rename = "COPIES_FROM")]
    CopiesFrom,
    #[serde(rename = "BUILDS")]
    Builds,
    #[serde(rename = "SUPERVISES")]
    Supervises,
    #[serde(rename = "SHIMS_FOR")]
    ShimsFor,
    #[serde(rename = "SMOKES_BY")]
    SmokesBy,
}

/// A composition entity (SPEC §6 `kernels/<name>/kernel.yaml`).
#[derive(Debug, Clone, Deserialize)]
pub struct Entity {
    pub name: String,
    #[serde(rename = "type")]
    pub entity_type: EntityType,
    #[serde(default)]
    pub version: Option<String>,
    /// SPEC v0.2 §B — the C1–C4 placement-layer digest, set when this entity is
    /// stackable without a build. A `placement_layer` on every consumed entity
    /// unlocks the manifest-assembly output mode (no `docker build`).
    #[serde(default)]
    pub placement_layer: Option<String>,
}

/// A directed predicate instance (SPEC §6 `links/<subj>.<PRED>.<obj>.yaml`).
#[derive(Debug, Clone, Deserialize)]
pub struct Link {
    pub subject: String,
    pub predicate: Predicate,
    pub object: String,
}

/// One bundle's composition graph.
#[derive(Debug, Default)]
pub struct Composition {
    pub entities: Vec<Entity>,
    pub links: Vec<Link>,
}

impl Composition {
    /// Load a `<bundle>.composition/` directory (SPEC §6: `COMPOSE.yaml` +
    /// `kernels/**/kernel.yaml` + `links/*.yaml` + `notifies/`).
    ///
    /// v0.0.1 scaffold: returns an empty graph so the CLI runs end-to-end. The
    /// real walk + serde parse (and the hard-error on a mis-named file) is the
    /// first task of milestone **M2**.
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        // TODO(M2): walk kernels/ + links/; parse with serde_yaml; reject any
        // file in kernels/ not named kernel.yaml and any link not matching
        // *.<PREDICATE>.*.yaml (SPEC §6).
        let _ = dir;
        Ok(Composition::default())
    }
}
