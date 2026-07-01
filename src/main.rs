//! sporaxis — ontology-first OCI bundle assembler.
//!
//! Declaration in (a closed-set composition ontology: five entity types, six
//! predicates) → everything physical out: a `Dockerfile` *or* a referenced OCI
//! manifest, `bundle.yaml`, s6 service trees, smoke scripts, a CHECKLIST, and
//! `composition.ttl` (the bill of materials as RDF). The ontological output is as
//! first-class as the image it produces — ontology-first build.
//!
//! `oci-germination` remains the orchestrator + master of bundle materialisation;
//! `sporaxis` is the engine it drives. v0.0.1 is the scaffold: the CLI and the
//! ontology types are real; the parser/invariants/emitters land at M2 (reproduce
//! oci-germination's current ck-allinone bundle byte-for-byte).

// Scaffold: the ontology carries fields the parser/emitters consume at M2 but
// nothing reads yet. Lift this allow as each is wired in.
#![allow(dead_code)]

mod invariants;
mod ontology;
mod render;
mod ttl;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "sporaxis",
    version,
    about = "Ontology-first OCI bundle assembler"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Compose a bundle's physical outputs from its composition directory.
    Compose {
        /// The `<bundle>.composition/` directory.
        dir: PathBuf,
        /// Output mode: `dockerfile` | `manifest` | `auto`.
        #[arg(long, default_value = "auto")]
        mode: String,
    },
    /// Parse + run the implemented invariants without emitting (the CI `compose --check`).
    Check {
        /// The `<bundle>.composition/` directory.
        dir: PathBuf,
    },
}

fn main() -> anyhow::Result<()> {
    match Cli::parse().cmd {
        Cmd::Compose { dir, mode } => {
            let comp = ontology::Composition::load(&dir)?;
            invariants::check_all(&comp)?;
            render::emit(&comp, &mode, &dir)?;
        }
        Cmd::Check { dir } => {
            let comp = ontology::Composition::load(&dir)?;
            invariants::check_all(&comp)?;
            println!(
                "ok: {} entities, {} links — invariants pass (I1–I6, I8, I9)",
                comp.entities.len(),
                comp.links.len()
            );
        }
    }
    Ok(())
}
