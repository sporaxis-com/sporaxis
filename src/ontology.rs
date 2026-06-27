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
    /// Set from the kernel's directory name (SPEC §6), not the yaml body.
    #[serde(default)]
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

/// The parsed YAML body of a link (SPEC §6 edge metadata). Fields vary by
/// predicate; unknown keys are kept in `rest` so emitters and later invariants can
/// read them. An empty body (`{}`) deserialises to all-defaults.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LinkMeta {
    /// SHIMS_FOR: the NOTIFY this shim is tracked by, in public form (I3).
    #[serde(default)]
    pub notify: Option<String>,
    /// SHIMS_FOR: the retire-when probe + behaviour (I4). Block or string.
    #[serde(default)]
    pub retire_when: Option<serde_yaml::Value>,
    /// COPIES_FROM: justification for a duplicate pull (I6 exemption).
    #[serde(default)]
    pub reason: Option<String>,
    /// SHIMS_FOR: human note on why the shim exists.
    #[serde(default)]
    pub because: Option<String>,
    /// Any other edge metadata (paths, mount_at, run, kind, package, …).
    #[serde(flatten, default)]
    pub rest: std::collections::BTreeMap<String, serde_yaml::Value>,
}

/// A directed predicate instance (SPEC §6 `links/<subj>.<PRED>.<obj>.yaml`).
#[derive(Debug, Clone)]
pub struct Link {
    pub subject: String,
    pub predicate: Predicate,
    pub object: String,
    /// The parsed link-file body (edge metadata); all-defaults when the file is `{}`.
    pub meta: LinkMeta,
}

impl Link {
    /// Parse `<subject>.<PREDICATE>.<object>` from a link filename stem. The
    /// predicate is one of the six known tokens; subject/object are kebab-case
    /// (so we split on the predicate, not on every `.`).
    fn from_filename(stem: &str) -> anyhow::Result<Self> {
        const PREDS: [(&str, Predicate); 6] = [
            ("INHERITS_FROM", Predicate::InheritsFrom),
            ("COPIES_FROM", Predicate::CopiesFrom),
            ("BUILDS", Predicate::Builds),
            ("SUPERVISES", Predicate::Supervises),
            ("SHIMS_FOR", Predicate::ShimsFor),
            ("SMOKES_BY", Predicate::SmokesBy),
        ];
        for (tok, pred) in PREDS {
            let pat = format!(".{tok}.");
            if let Some(i) = stem.find(&pat) {
                return Ok(Link {
                    subject: stem[..i].to_string(),
                    predicate: pred,
                    object: stem[i + pat.len()..].to_string(),
                    meta: LinkMeta::default(),
                });
            }
        }
        anyhow::bail!(
            "link '{stem}' has no recognised predicate \
             (expected <subject>.<PREDICATE>.<object>.yaml)"
        )
    }
}

/// One bundle's composition graph.
#[derive(Debug, Default)]
pub struct Composition {
    pub entities: Vec<Entity>,
    pub links: Vec<Link>,
}

impl Composition {
    /// Load a `<bundle>.composition/` directory (SPEC §6): `kernels/<name>/kernel.yaml`
    /// (one entity; `name` = the directory) + `links/<subj>.<PREDICATE>.<obj>.yaml`
    /// (one predicate instance; subject/predicate/object from the filename). Reads
    /// in sorted order so the downstream render is deterministic (SPEC §4).
    pub fn load(dir: &Path) -> anyhow::Result<Self> {
        let mut entities = Vec::new();
        let kernels = dir.join("kernels");
        if kernels.is_dir() {
            let mut kdirs: Vec<_> = std::fs::read_dir(&kernels)?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect();
            kdirs.sort();
            for kdir in kdirs {
                let ky = kdir.join("kernel.yaml");
                let name = kdir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                anyhow::ensure!(ky.exists(), "kernels/{name}: missing kernel.yaml");
                let mut e: Entity = serde_yaml::from_reader(std::fs::File::open(&ky)?)
                    .map_err(|err| anyhow::anyhow!("{}: {err}", ky.display()))?;
                e.name = name;
                entities.push(e);
            }
        }

        let mut links = Vec::new();
        let linksdir = dir.join("links");
        if linksdir.is_dir() {
            let mut files: Vec<_> = std::fs::read_dir(&linksdir)?
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("yaml"))
                .collect();
            files.sort();
            for f in files {
                let stem = f.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                let mut link = Link::from_filename(stem)?;
                let body = std::fs::read_to_string(&f)?;
                if !body.trim().is_empty() {
                    link.meta = serde_yaml::from_str(&body)
                        .map_err(|err| anyhow::anyhow!("{}: {err}", f.display()))?;
                }
                links.push(link);
            }
        }

        Ok(Composition { entities, links })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_example_composition() {
        let dir =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/hello.composition");
        let comp = Composition::load(&dir).expect("load");
        assert_eq!(comp.entities.len(), 3, "3 kernels");
        assert_eq!(comp.links.len(), 2, "2 links");
        assert!(comp
            .entities
            .iter()
            .any(|e| e.name == "ck-allinone" && matches!(e.entity_type, EntityType::FleetImage)));
        assert!(comp
            .entities
            .iter()
            .any(|e| e.name == "pgrdf" && e.placement_layer.is_some()));
        assert!(comp
            .links
            .iter()
            .any(|l| l.subject == "ck-allinone" && matches!(l.predicate, Predicate::InheritsFrom)));
    }

    #[test]
    fn link_filename_splits_on_the_predicate_not_every_dot() {
        let l = Link::from_filename("ck-allinone.COPIES_FROM.pgrdf").unwrap();
        assert_eq!(l.subject, "ck-allinone");
        assert_eq!(l.object, "pgrdf");
        assert!(matches!(l.predicate, Predicate::CopiesFrom));
        assert!(Link::from_filename("no-predicate-here").is_err());
    }
}
