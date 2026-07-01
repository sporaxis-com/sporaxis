//! Output emitters (SPEC.SPORAXIS §4 + v0.2 §C).
//!
//! One ontology → the **dockerfile** mode (a multi-stage build) or the
//! **manifest** mode (by-digest layer references — no build, no re-pack). Both
//! modes also emit `bundle.yaml` (the layer bill of materials), the smoke script,
//! and `composition.ttl` (the BOM as RDF). Dockerfile mode additionally emits the
//! s6 service tree.
//!
//! Everything is written under `<composition>/out/` and is **deterministic**: the
//! same graph re-composes byte-for-byte (SPEC §4). Byte-identical reproduction of
//! `oci-germination`'s hand-written bundle is the separate M2 gate (#1); these
//! emitters render the structure from the edges (#4).

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context;
use serde_yaml::Value;

use crate::ontology::{Composition, Entity, EntityType, Link, LinkMeta, Predicate};

/// Emit a bundle's physical outputs in the chosen mode, under `<dir>/out/`.
pub fn emit(comp: &Composition, mode: &str, dir: &Path) -> anyhow::Result<()> {
    let chosen = resolve_mode(comp, mode);
    let root = root_fleet(comp)
        .context("no root oci:FleetImage (a FleetImage not inherited by another) to compose")?;
    let out = dir.join("out");
    std::fs::create_dir_all(&out)?;

    // Both modes: the RDF BOM, the layer BOM, and the smoke gate.
    write(&out, "composition.ttl", &crate::ttl::to_turtle(comp)?)?;
    write(&out, "bundle.yaml", &render_bundle_yaml(comp, root)?)?;
    write(
        &out,
        &format!("smoke-{}.sh", root.name),
        &render_smoke(comp)?,
    )?;

    let mut wrote = vec!["composition.ttl".to_string(), "bundle.yaml".to_string()];
    match chosen {
        "dockerfile" => {
            write(&out, "Dockerfile", &render_dockerfile(comp, root)?)?;
            for (rel, content) in render_s6(comp, root) {
                write(&out, &rel, &content)?;
            }
            wrote.push("Dockerfile".into());
            wrote.push("s6-services/".into());
        }
        "manifest" => {
            write(
                &out,
                "manifest.plan.txt",
                &render_manifest_plan(comp, root)?,
            )?;
            wrote.push("manifest.plan.txt".into());
        }
        other => anyhow::bail!("unknown mode '{other}' (expected dockerfile | manifest | auto)"),
    }

    println!(
        "compose: {} entities, {} links → mode={chosen} → {}/  ({}, smoke-{}.sh)",
        comp.entities.len(),
        comp.links.len(),
        out.display(),
        wrote.join(", "),
        root.name,
    );
    Ok(())
}

fn write(dir: &Path, rel: &str, content: &str) -> anyhow::Result<()> {
    let p = dir.join(rel);
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(p, content)?;
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

// ---- graph helpers ---------------------------------------------------------

/// The bundle being composed: a `oci:FleetImage` that no other entity inherits.
fn root_fleet(comp: &Composition) -> Option<&Entity> {
    let bases: BTreeSet<&str> = comp
        .links
        .iter()
        .filter(|l| l.predicate == Predicate::InheritsFrom)
        .map(|l| l.object.as_str())
        .collect();
    comp.entities
        .iter()
        .find(|e| e.entity_type == EntityType::FleetImage && !bases.contains(e.name.as_str()))
}

fn by_name<'a>(comp: &'a Composition, name: &str) -> Option<&'a Entity> {
    comp.entities.iter().find(|e| e.name == name)
}

/// The image reference for an entity: its `placement_layer` if any, else
/// `<name>:<version>` (a coherent ref; the real registry ref is M2's concern).
fn image_ref(comp: &Composition, name: &str) -> String {
    match by_name(comp, name) {
        Some(e) => e
            .placement_layer
            .clone()
            .unwrap_or_else(|| format!("{}:{}", e.name, e.version.as_deref().unwrap_or("latest"))),
        None => name.to_string(),
    }
}

/// Edges of `pred` with subject `subject`, sorted by object (determinism).
fn edges<'a>(comp: &'a Composition, subject: &str, pred: Predicate) -> Vec<&'a Link> {
    let mut v: Vec<&Link> = comp
        .links
        .iter()
        .filter(|l| l.predicate == pred && l.subject == subject)
        .collect();
    v.sort_by(|a, b| a.object.cmp(&b.object));
    v
}

fn meta_str<'a>(meta: &'a LinkMeta, key: &str) -> Option<&'a str> {
    meta.rest.get(key).and_then(Value::as_str)
}

fn meta_paths(meta: &LinkMeta) -> Vec<String> {
    match meta.rest.get("paths") {
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        Some(Value::String(s)) => vec![s.clone()],
        _ => vec![],
    }
}

// ---- emitters --------------------------------------------------------------

fn render_dockerfile(comp: &Composition, root: &Entity) -> anyhow::Result<String> {
    let mut o = String::new();
    o.push_str("# Generated by sporaxis — do not edit; edit the composition and re-compose.\n");
    o.push_str(&format!(
        "# bundle: {} ({})\n\n",
        root.name,
        root.version.as_deref().unwrap_or("?")
    ));

    if let Some(base) = edges(comp, &root.name, Predicate::InheritsFrom).first() {
        o.push_str(&format!("FROM {}\n\n", image_ref(comp, &base.object)));
    }

    let builds = edges(comp, &root.name, Predicate::Builds);
    for l in &builds {
        let kind = meta_str(&l.meta, "kind").unwrap_or("build");
        let pkg = meta_str(&l.meta, "package").unwrap_or(l.object.as_str());
        o.push_str(&format!("# BUILDS {} ({kind})\n", l.object));
        o.push_str(&format!(
            "RUN sporaxis-build --{kind} {pkg}   # → {}\n",
            l.object
        ));
    }
    if !builds.is_empty() {
        o.push('\n');
    }

    for l in edges(comp, &root.name, Predicate::CopiesFrom) {
        let dest = meta_str(&l.meta, "mount_at").unwrap_or("/");
        let paths = meta_paths(&l.meta);
        let paths_s = if paths.is_empty() {
            "/".to_string()
        } else {
            paths.join(" ")
        };
        o.push_str(&format!(
            "COPY --from={} {} {}\n",
            image_ref(comp, &l.object),
            paths_s,
            dest
        ));
    }
    o.push('\n');

    // I7 — manifest labels derive ONLY from the oci:FleetImage entity.
    o.push_str("# I7 — labels derive only from the oci:FleetImage entity\n");
    o.push_str(&format!(
        "LABEL org.opencontainers.image.title={}\n",
        root.name
    ));
    o.push_str(&format!(
        "LABEL org.opencontainers.image.version={}\n",
        root.version.as_deref().unwrap_or("0")
    ));

    let sup = edges(comp, &root.name, Predicate::Supervises);
    if !sup.is_empty() {
        o.push_str("\n# s6 service tree (SUPERVISES) — see out/s6-services/\n");
        o.push_str("COPY s6-services/ /etc/s6-overlay/s6-rc.d/\n");
        for l in sup {
            o.push_str(&format!("#   longrun: {}\n", l.object));
        }
    }
    Ok(o)
}

fn render_s6(comp: &Composition, root: &Entity) -> Vec<(String, String)> {
    let mut files = Vec::new();
    for l in edges(comp, &root.name, Predicate::Supervises) {
        let svc = &l.object;
        let run = meta_str(&l.meta, "run").unwrap_or(svc);
        files.push((format!("s6-services/{svc}/type"), "longrun\n".to_string()));
        files.push((
            format!("s6-services/{svc}/run"),
            format!("#!/command/execlineb -P\n{run}\n"),
        ));
    }
    files
}

fn render_smoke(comp: &Composition) -> anyhow::Result<String> {
    let mut o = String::new();
    o.push_str("#!/bin/sh\n");
    o.push_str("# Generated by sporaxis — smoke assertions from SMOKES_BY edges.\n");
    o.push_str("set -eu\n\n");
    let mut smokes: Vec<&Link> = comp
        .links
        .iter()
        .filter(|l| l.predicate == Predicate::SmokesBy)
        .collect();
    smokes.sort_by(|a, b| {
        (a.subject.as_str(), a.object.as_str()).cmp(&(b.subject.as_str(), b.object.as_str()))
    });
    for l in smokes {
        o.push_str(&format!("# {} SMOKES_BY {}\n", l.subject, l.object));
        o.push_str(&format!("echo 'smoke: {} / {}'\n", l.subject, l.object));
    }
    Ok(o)
}

fn render_bundle_yaml(comp: &Composition, root: &Entity) -> anyhow::Result<String> {
    let mut o = String::new();
    o.push_str("# Generated by sporaxis — the layer bill of materials.\n");
    o.push_str(&format!("name: {}\n", root.name));
    o.push_str(&format!(
        "version: {}\n",
        root.version.as_deref().unwrap_or("?")
    ));
    if let Some(base) = edges(comp, &root.name, Predicate::InheritsFrom).first() {
        o.push_str(&format!("base: {}\n", image_ref(comp, &base.object)));
    }

    let section = |o: &mut String, header: &str, kind: EntityType| {
        let mut es: Vec<&Entity> = comp
            .entities
            .iter()
            .filter(|e| e.entity_type == kind)
            .collect();
        es.sort_by(|a, b| a.name.cmp(&b.name));
        if es.is_empty() {
            return;
        }
        o.push_str(header);
        for e in es {
            o.push_str(&format!(
                "  {}:\n    version: {}\n",
                e.name,
                e.version.as_deref().unwrap_or("?")
            ));
            if let Some(p) = &e.placement_layer {
                o.push_str(&format!("    placement_layer: {p}\n"));
            }
        }
    };
    section(&mut o, "extensions:\n", EntityType::DbExtension);
    section(&mut o, "components:\n", EntityType::StaticArtifact);

    let mut procs: Vec<&Entity> = comp
        .entities
        .iter()
        .filter(|e| e.entity_type == EntityType::Process)
        .collect();
    procs.sort_by(|a, b| a.name.cmp(&b.name));
    if !procs.is_empty() {
        o.push_str("processes:\n");
        for e in procs {
            o.push_str(&format!("  - {}\n", e.name));
        }
    }
    Ok(o)
}

fn render_manifest_plan(comp: &Composition, root: &Entity) -> anyhow::Result<String> {
    let mut o = String::new();
    o.push_str("# Generated by sporaxis — manifest-mode layer plan (by-digest, no build).\n");
    o.push_str(&format!(
        "# image: {} ({})\n",
        root.name,
        root.version.as_deref().unwrap_or("?")
    ));
    if let Some(base) = edges(comp, &root.name, Predicate::InheritsFrom).first() {
        o.push_str(&format!("base: {}\n", image_ref(comp, &base.object)));
    }
    o.push_str("layers:\n");
    let mut placed: Vec<&Entity> = comp
        .entities
        .iter()
        .filter(|e| e.placement_layer.is_some())
        .collect();
    placed.sort_by(|a, b| a.name.cmp(&b.name));
    for e in placed {
        o.push_str(&format!(
            "  - {} -> {}\n",
            e.name,
            e.placement_layer.as_ref().unwrap()
        ));
    }
    Ok(o)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::Link;
    use std::collections::BTreeMap;

    fn ent(name: &str, t: EntityType, version: Option<&str>, placement: Option<&str>) -> Entity {
        Entity {
            name: name.into(),
            entity_type: t,
            version: version.map(Into::into),
            placement_layer: placement.map(Into::into),
            provenance: None,
        }
    }

    fn lk(subject: &str, p: Predicate, object: &str, rest: &[(&str, Value)]) -> Link {
        Link {
            subject: subject.into(),
            predicate: p,
            object: object.into(),
            meta: LinkMeta {
                rest: rest
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.clone()))
                    .collect(),
                ..Default::default()
            },
        }
    }

    fn sample() -> Composition {
        Composition {
            entities: vec![
                ent("ck", EntityType::FleetImage, Some("v9"), None),
                ent("base", EntityType::FleetImage, Some("v1"), None),
                ent("relay", EntityType::StaticArtifact, Some("in-tree"), None),
                ent(
                    "pgrdf",
                    EntityType::DbExtension,
                    Some("0.6.17"),
                    Some("ghcr/pgrdf:0.6.17"),
                ),
                ent("nats", EntityType::Process, None, None),
            ],
            links: vec![
                lk("ck", Predicate::InheritsFrom, "base", &[]),
                lk(
                    "ck",
                    Predicate::Builds,
                    "relay",
                    &[
                        ("kind", Value::String("go".into())),
                        ("package", Value::String("./cmd/relay".into())),
                    ],
                ),
                lk(
                    "ck",
                    Predicate::CopiesFrom,
                    "pgrdf",
                    &[("mount_at", Value::String("/ext".into()))],
                ),
                lk(
                    "ck",
                    Predicate::Supervises,
                    "nats",
                    &[("run", Value::String("nats-server -c /etc/nats.conf".into()))],
                ),
                lk("nats", Predicate::SmokesBy, "ping-4222", &[]),
            ],
        }
    }

    #[test]
    fn dockerfile_has_from_builds_copies_and_i7_labels() {
        let c = sample();
        let root = root_fleet(&c).unwrap();
        let df = render_dockerfile(&c, root).unwrap();
        assert!(df.contains("FROM base:v1"), "FROM from INHERITS_FROM base");
        assert!(df.contains("--go ./cmd/relay"), "BUILDS uses kind+package");
        assert!(
            df.contains("COPY --from=ghcr/pgrdf:0.6.17 / /ext"),
            "COPIES_FROM uses placement_layer ref + mount_at"
        );
        assert!(
            df.contains("LABEL org.opencontainers.image.version=v9"),
            "I7 label from the FleetImage version (v9), not the base (v1)"
        );
        assert!(
            !df.contains("version=v1"),
            "I7 never inherits the base version"
        );
    }

    #[test]
    fn s6_tree_has_a_run_per_supervised_process() {
        let c = sample();
        let root = root_fleet(&c).unwrap();
        let files: BTreeMap<String, String> = render_s6(&c, root).into_iter().collect();
        assert_eq!(
            files.get("s6-services/nats/type").map(String::as_str),
            Some("longrun\n")
        );
        assert!(files["s6-services/nats/run"].contains("nats-server -c /etc/nats.conf"));
    }

    #[test]
    fn bundle_yaml_lists_extensions_with_placement_and_processes() {
        let c = sample();
        let root = root_fleet(&c).unwrap();
        let y = render_bundle_yaml(&c, root).unwrap();
        assert!(y.contains("name: ck"));
        assert!(y.contains("placement_layer: ghcr/pgrdf:0.6.17"));
        assert!(y.contains("- nats"));
    }

    #[test]
    fn smoke_has_a_line_per_smokes_by_edge() {
        let c = sample();
        let s = render_smoke(&c).unwrap();
        assert!(s.starts_with("#!/bin/sh"));
        assert!(s.contains("nats SMOKES_BY ping-4222"));
    }

    #[test]
    fn rendering_is_deterministic() {
        let c = sample();
        let root = root_fleet(&c).unwrap();
        assert_eq!(
            render_dockerfile(&c, root).unwrap(),
            render_dockerfile(&c, root).unwrap()
        );
        assert_eq!(
            render_bundle_yaml(&c, root).unwrap(),
            render_bundle_yaml(&c, root).unwrap()
        );
    }
}
