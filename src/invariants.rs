//! Discipline as graph constraints (SPEC.SPORAXIS §5 — I1–I8).
//!
//! The assembler refuses to emit any bundle that violates one. Each invariant
//! replaces a prose rule a reviewer used to hold in their head:
//!
//! - I1  no `oci:UpstreamImage` is the subject of `BUILDS`   (never compile upstream)
//! - I2  no `oci:UpstreamImage` is the subject of `SHIMS_FOR` (never patch upstream)
//! - I3  every `SHIMS_FOR` carries a `notify:` that resolves to a real NOTIFY
//! - I4  every `SHIMS_FOR` carries a `retire_when:` probe + behaviour
//! - I5  every `svc:Process` has a `SMOKES_BY` (or is explicitly `pure`)
//! - I6  duplicate-pull warning when a COPIES_FROM base is already inherited
//! - I7  manifest labels derive only from the `oci:FleetImage` entity
//! - I8  no committed output references a `.gitignore`'d path (e.g. `_WIP/`)

use crate::ontology::Composition;

/// Run I1–I8. Returns the first violation as an error (SPEC §5: non-zero exit
/// with a structured report — `<code> <subject> <predicate>? <object>? <reason>`).
///
/// v0.0.1 scaffold: passes trivially on the empty graph. The eight checks land at
/// milestone **M2**, validated against the worked example in SPEC.SPORAXIS §D.
pub fn check_all(comp: &Composition) -> anyhow::Result<()> {
    // TODO(M2): implement I1–I8 over comp.entities + comp.links; collect every
    // violation, print one per line, and return Err on a non-empty set.
    let _ = comp;
    Ok(())
}
