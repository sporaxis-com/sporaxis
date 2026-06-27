//! RDF emission via oxigraph (SPEC.SPORAXIS v0.2 §E — `composition.ttl`).
//!
//! The composition graph lives as triples in an oxigraph store; Turtle is just a
//! serialisation of it. This is the ontology-first payoff: the very graph the
//! Dockerfile/manifest is rendered from is itself a first-class RDF output —
//! loadable into pgRDF, governable by pgCK, validatable by SHACL.

use anyhow::Context;
use oxigraph::io::RdfFormat;
use oxigraph::model::{GraphName, GraphNameRef, Literal, NamedNode, Quad};
use oxigraph::store::Store;

use crate::ontology::{Composition, EntityType, Predicate};

/// The sporaxis composition vocabulary.
const SX: &str = "https://sporaxis.dev/ontology/compose#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

fn nn(iri: impl AsRef<str>) -> anyhow::Result<NamedNode> {
    NamedNode::new(iri.as_ref()).context("named node")
}

fn entity_class(t: EntityType) -> &'static str {
    match t {
        EntityType::UpstreamImage => "UpstreamImage",
        EntityType::FleetImage => "FleetImage",
        EntityType::StaticArtifact => "StaticArtifact",
        EntityType::DbExtension => "DBExtension",
        EntityType::Process => "Process",
    }
}

fn predicate_term(p: Predicate) -> &'static str {
    match p {
        Predicate::InheritsFrom => "INHERITS_FROM",
        Predicate::CopiesFrom => "COPIES_FROM",
        Predicate::Builds => "BUILDS",
        Predicate::Supervises => "SUPERVISES",
        Predicate::ShimsFor => "SHIMS_FOR",
        Predicate::SmokesBy => "SMOKES_BY",
    }
}

/// Build the composition as an oxigraph graph and serialise it as Turtle.
pub fn to_turtle(comp: &Composition) -> anyhow::Result<String> {
    let store = Store::new().context("oxigraph store")?;
    let rdf_type = nn(RDF_TYPE)?;
    let sx_version = nn(format!("{SX}version"))?;
    let sx_placement = nn(format!("{SX}placementLayer"))?;

    for e in &comp.entities {
        let subj = nn(format!("{SX}{}", e.name))?;
        let class = nn(format!("{SX}{}", entity_class(e.entity_type)))?;
        store.insert(&Quad::new(
            subj.clone(),
            rdf_type.clone(),
            class,
            GraphName::DefaultGraph,
        ))?;
        if let Some(v) = &e.version {
            store.insert(&Quad::new(
                subj.clone(),
                sx_version.clone(),
                Literal::new_simple_literal(v.as_str()),
                GraphName::DefaultGraph,
            ))?;
        }
        if let Some(p) = &e.placement_layer {
            store.insert(&Quad::new(
                subj,
                sx_placement.clone(),
                Literal::new_simple_literal(p.as_str()),
                GraphName::DefaultGraph,
            ))?;
        }
    }

    for l in &comp.links {
        store.insert(&Quad::new(
            nn(format!("{SX}{}", l.subject))?,
            nn(format!("{SX}{}", predicate_term(l.predicate)))?,
            nn(format!("{SX}{}", l.object))?,
            GraphName::DefaultGraph,
        ))?;
    }

    let mut buf = Vec::new();
    store
        .dump_graph_to_writer(GraphNameRef::DefaultGraph, RdfFormat::Turtle, &mut buf)
        .context("turtle dump")?;
    Ok(String::from_utf8(buf)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{Entity, Link};

    #[test]
    fn emits_turtle_for_a_small_graph() {
        let comp = Composition {
            entities: vec![
                Entity {
                    name: "ck-allinone".into(),
                    entity_type: EntityType::FleetImage,
                    version: Some("v0.7.22".into()),
                    placement_layer: None,
                },
                Entity {
                    name: "pgrdf".into(),
                    entity_type: EntityType::DbExtension,
                    version: Some("0.6.17".into()),
                    placement_layer: Some("sha256:abc".into()),
                },
            ],
            links: vec![Link {
                subject: "ck-allinone".into(),
                predicate: Predicate::InheritsFrom,
                object: "pg-base".into(),
                meta: Default::default(),
            }],
        };
        let ttl = to_turtle(&comp).expect("turtle");
        assert!(ttl.contains("ck-allinone"), "subject present");
        assert!(ttl.contains("0.6.17"), "version literal present");
        assert!(ttl.contains("INHERITS_FROM"), "predicate present");
        assert!(ttl.contains("placementLayer"), "placement layer present");
    }
}
