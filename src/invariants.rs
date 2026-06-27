//! Discipline as graph constraints (SPEC.SPORAXIS §5 — I1–I8).
//!
//! The assembler refuses to emit any bundle that violates a hard invariant. Each
//! one replaces a prose rule a reviewer used to hold in their head:
//!
//! - I1  no `oci:UpstreamImage` is the subject of `BUILDS`   (never compile upstream)   [error]
//! - I2  no `oci:UpstreamImage` is the subject of `SHIMS_FOR` (never patch upstream)     [error]
//! - I5  every `svc:Process` has a `SMOKES_BY` (`… none` ⇒ classified `pure`)            [error/warn]
//! - I6  duplicate-pull: COPIES_FROM(x, upstream) when x already inherits that upstream  [warn]
//!
//! Implemented here at the graph level. The remaining invariants depend on inputs
//! this stage does not yet have, and are tracked separately:
//!
//! - I3/I4  `notify:` / `retire_when:` on every `SHIMS_FOR` — need the link YAML body (not yet retained by the parser).
//! - I8  no committed output references a `.gitignore`'d path — needs link bodies + gitignore awareness.
//! - I7  manifest labels derive only from the `oci:FleetImage` entity — an emit-time rule for the renderer (M2), not a pre-emit check.

use std::collections::{BTreeMap, BTreeSet};

use crate::ontology::{Composition, EntityType, Predicate};

/// Whether a violation blocks emission (`Error`) or is advisory (`Warning`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// One invariant violation, rendered as SPEC §5's structured line:
/// `<code> <subject> <predicate>? <object>? <reason>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub code: &'static str,
    pub severity: Severity,
    pub subject: Option<String>,
    pub predicate: Option<Predicate>,
    pub object: Option<String>,
    pub reason: String,
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.code)?;
        if let Some(s) = &self.subject {
            write!(f, " {s}")?;
        }
        if let Some(p) = self.predicate {
            write!(f, " {}", predicate_token(p))?;
        }
        if let Some(o) = &self.object {
            write!(f, " {o}")?;
        }
        write!(f, ": {}", self.reason)
    }
}

/// The outcome of running I1–I8 over a composition graph.
#[derive(Debug, Default)]
pub struct Report {
    pub violations: Vec<Violation>,
}

impl Report {
    pub fn errors(&self) -> impl Iterator<Item = &Violation> {
        self.violations
            .iter()
            .filter(|v| v.severity == Severity::Error)
    }

    pub fn warnings(&self) -> impl Iterator<Item = &Violation> {
        self.violations
            .iter()
            .filter(|v| v.severity == Severity::Warning)
    }

    pub fn has_errors(&self) -> bool {
        self.errors().next().is_some()
    }
}

/// Is `target` reachable from `start` by following `INHERITS_FROM` edges
/// (at least one hop)? `adj` maps subject → its direct `INHERITS_FROM` objects.
fn inherits_reaches<'a>(start: &str, target: &str, adj: &BTreeMap<&'a str, Vec<&'a str>>) -> bool {
    let mut stack: Vec<&'a str> = adj.get(start).cloned().unwrap_or_default();
    let mut seen: BTreeSet<&'a str> = BTreeSet::new();
    while let Some(n) = stack.pop() {
        if n == target {
            return true;
        }
        if seen.insert(n) {
            if let Some(next) = adj.get(n) {
                stack.extend(next.iter().copied());
            }
        }
    }
    false
}

fn predicate_token(p: Predicate) -> &'static str {
    match p {
        Predicate::InheritsFrom => "INHERITS_FROM",
        Predicate::CopiesFrom => "COPIES_FROM",
        Predicate::Builds => "BUILDS",
        Predicate::Supervises => "SUPERVISES",
        Predicate::ShimsFor => "SHIMS_FOR",
        Predicate::SmokesBy => "SMOKES_BY",
    }
}

/// Run the implemented invariants over the graph and collect every violation.
/// Pure (no IO) so it is directly testable; [`check_all`] wraps it for the CLI.
pub fn evaluate(comp: &Composition) -> Report {
    let mut report = Report::default();
    let types: BTreeMap<&str, EntityType> = comp
        .entities
        .iter()
        .map(|e| (e.name.as_str(), e.entity_type))
        .collect();
    let is_upstream = |name: &str| types.get(name) == Some(&EntityType::UpstreamImage);

    for l in &comp.links {
        // I1 — never compile an upstream image.
        if l.predicate == Predicate::Builds && is_upstream(&l.subject) {
            report.violations.push(Violation {
                code: "I1",
                severity: Severity::Error,
                subject: Some(l.subject.clone()),
                predicate: Some(l.predicate),
                object: Some(l.object.clone()),
                reason: "oci:UpstreamImage must not be the subject of BUILDS \
                         (never compile an upstream image)"
                    .into(),
            });
        }

        // I2 — never patch an upstream image; a shim attaches, it does not modify.
        if l.predicate == Predicate::ShimsFor && is_upstream(&l.subject) {
            report.violations.push(Violation {
                code: "I2",
                severity: Severity::Error,
                subject: Some(l.subject.clone()),
                predicate: Some(l.predicate),
                object: Some(l.object.clone()),
                reason: "oci:UpstreamImage must not be the subject of SHIMS_FOR \
                         (never patch an upstream image — file a NOTIFY instead)"
                    .into(),
            });
        }
    }

    // I5 — every svc:Process must be smoked; a `SMOKES_BY … none` edge classifies
    // it `pure` (no observable runtime surface) and is warned on, not blocked.
    for e in &comp.entities {
        if e.entity_type != EntityType::Process {
            continue;
        }
        let smokes: Vec<&str> = comp
            .links
            .iter()
            .filter(|l| l.predicate == Predicate::SmokesBy && l.subject == e.name)
            .map(|l| l.object.as_str())
            .collect();
        if smokes.is_empty() {
            report.violations.push(Violation {
                code: "I5",
                severity: Severity::Error,
                subject: Some(e.name.clone()),
                predicate: None,
                object: None,
                reason: "svc:Process has no SMOKES_BY edge \
                         (add one, or classify it `pure` with `SMOKES_BY none`)"
                    .into(),
            });
        } else if smokes.contains(&"none") {
            report.violations.push(Violation {
                code: "I5",
                severity: Severity::Warning,
                subject: Some(e.name.clone()),
                predicate: Some(Predicate::SmokesBy),
                object: Some("none".into()),
                reason: "svc:Process classified `pure` (no observable runtime surface)".into(),
            });
        }
    }

    // I6 — duplicate pull: copying from an oci:UpstreamImage that the entity already
    // inherits transitively (the COPY re-pulls what the base chain already carries).
    let mut inherits: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for l in &comp.links {
        if l.predicate == Predicate::InheritsFrom {
            inherits
                .entry(l.subject.as_str())
                .or_default()
                .push(l.object.as_str());
        }
    }
    for l in &comp.links {
        if l.predicate != Predicate::CopiesFrom || !is_upstream(&l.object) {
            continue;
        }
        let already_inherited = comp.links.iter().any(|a| {
            a.predicate == Predicate::InheritsFrom
                && a.subject == l.subject
                && inherits_reaches(&a.object, &l.object, &inherits)
        });
        if already_inherited {
            report.violations.push(Violation {
                code: "I6",
                severity: Severity::Warning,
                subject: Some(l.subject.clone()),
                predicate: Some(Predicate::CopiesFrom),
                object: Some(l.object.clone()),
                reason: "duplicate pull — COPIES_FROM an oci:UpstreamImage already inherited \
                         transitively; justify it or NOTIFY the intermediate ancestor"
                    .into(),
            });
        }
    }

    report
}

/// Run I1–I8 and return `Err` if any hard invariant is violated, after printing a
/// structured report (one line per violation) to stderr. Warnings never block.
pub fn check_all(comp: &Composition) -> anyhow::Result<()> {
    let report = evaluate(comp);
    for w in report.warnings() {
        eprintln!("warning: {w}");
    }
    if report.has_errors() {
        for e in report.errors() {
            eprintln!("{e}");
        }
        let n = report.errors().count();
        anyhow::bail!("{n} invariant violation(s) — refusing to emit");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::{Entity, Link};

    fn entity(name: &str, t: EntityType) -> Entity {
        Entity {
            name: name.into(),
            entity_type: t,
            version: None,
            placement_layer: None,
        }
    }

    fn link(subject: &str, predicate: Predicate, object: &str) -> Link {
        Link {
            subject: subject.into(),
            predicate,
            object: object.into(),
        }
    }

    #[test]
    fn i1_flags_an_upstream_image_built_by_a_builds_edge() {
        let comp = Composition {
            entities: vec![entity("postgres-bookworm", EntityType::UpstreamImage)],
            links: vec![link("postgres-bookworm", Predicate::Builds, "patched-pg")],
        };
        let report = evaluate(&comp);
        let i1: Vec<_> = report.errors().filter(|v| v.code == "I1").collect();
        assert_eq!(i1.len(), 1, "one I1 error for the upstream BUILDS edge");
        assert_eq!(i1[0].subject.as_deref(), Some("postgres-bookworm"));
    }

    #[test]
    fn i1_allows_a_fleet_image_to_build() {
        let comp = Composition {
            entities: vec![entity("ck-allinone", EntityType::FleetImage)],
            links: vec![link("ck-allinone", Predicate::Builds, "relay")],
        };
        assert_eq!(
            evaluate(&comp).errors().filter(|v| v.code == "I1").count(),
            0
        );
    }

    #[test]
    fn i2_flags_an_upstream_image_shimmed_by_a_shims_for_edge() {
        let comp = Composition {
            entities: vec![entity("postgres-bookworm", EntityType::UpstreamImage)],
            links: vec![link("postgres-bookworm", Predicate::ShimsFor, "pg-quirk")],
        };
        let report = evaluate(&comp);
        let i2: Vec<_> = report.errors().filter(|v| v.code == "I2").collect();
        assert_eq!(i2.len(), 1, "one I2 error for the upstream SHIMS_FOR edge");
        assert_eq!(i2[0].subject.as_deref(), Some("postgres-bookworm"));
    }

    #[test]
    fn i5_flags_a_process_with_no_smoke() {
        let comp = Composition {
            entities: vec![entity("relay", EntityType::Process)],
            links: vec![],
        };
        let report = evaluate(&comp);
        let i5: Vec<_> = report.errors().filter(|v| v.code == "I5").collect();
        assert_eq!(i5.len(), 1, "one I5 error for the unsmoked process");
        assert_eq!(i5[0].subject.as_deref(), Some("relay"));
    }

    #[test]
    fn i5_passes_a_process_with_a_real_smoke() {
        let comp = Composition {
            entities: vec![entity("postgres-server", EntityType::Process)],
            links: vec![link("postgres-server", Predicate::SmokesBy, "pg_isready")],
        };
        let report = evaluate(&comp);
        assert_eq!(report.errors().filter(|v| v.code == "I5").count(), 0);
        assert_eq!(report.warnings().filter(|v| v.code == "I5").count(), 0);
    }

    #[test]
    fn i5_warns_on_a_pure_process_smoked_by_none() {
        let comp = Composition {
            entities: vec![entity("control-only", EntityType::Process)],
            links: vec![link("control-only", Predicate::SmokesBy, "none")],
        };
        let report = evaluate(&comp);
        assert_eq!(report.errors().filter(|v| v.code == "I5").count(), 0);
        let warns: Vec<_> = report.warnings().filter(|v| v.code == "I5").collect();
        assert_eq!(warns.len(), 1, "one I5 warning for the pure classification");
    }

    #[test]
    fn i6_warns_when_copying_an_upstream_already_inherited_transitively() {
        // app inherits mid, mid inherits base (upstream) — and app ALSO copies base.
        let comp = Composition {
            entities: vec![
                entity("app", EntityType::FleetImage),
                entity("mid", EntityType::FleetImage),
                entity("base", EntityType::UpstreamImage),
            ],
            links: vec![
                link("app", Predicate::InheritsFrom, "mid"),
                link("mid", Predicate::InheritsFrom, "base"),
                link("app", Predicate::CopiesFrom, "base"),
            ],
        };
        let report = evaluate(&comp);
        let i6: Vec<_> = report.warnings().filter(|v| v.code == "I6").collect();
        assert_eq!(i6.len(), 1, "one I6 duplicate-pull warning");
        assert_eq!(i6[0].subject.as_deref(), Some("app"));
        assert_eq!(i6[0].object.as_deref(), Some("base"));
        assert!(!report.has_errors(), "I6 is advisory, not an error");
    }

    #[test]
    fn i6_silent_when_the_copied_upstream_is_not_already_inherited() {
        // app copies base, but its inherited chain does not include base.
        let comp = Composition {
            entities: vec![
                entity("app", EntityType::FleetImage),
                entity("other", EntityType::FleetImage),
                entity("base", EntityType::UpstreamImage),
            ],
            links: vec![
                link("app", Predicate::InheritsFrom, "other"),
                link("app", Predicate::CopiesFrom, "base"),
            ],
        };
        assert_eq!(
            evaluate(&comp)
                .warnings()
                .filter(|v| v.code == "I6")
                .count(),
            0
        );
    }

    #[test]
    fn a_clean_graph_has_no_violations() {
        // Shaped like the shipped example: a fleet image inheriting another and
        // copying a placement-layer extension — no upstream build/patch/dup-pull.
        let comp = Composition {
            entities: vec![
                entity("ck-allinone", EntityType::FleetImage),
                entity("pg-base", EntityType::FleetImage),
                entity("pgrdf", EntityType::DbExtension),
            ],
            links: vec![
                link("ck-allinone", Predicate::InheritsFrom, "pg-base"),
                link("ck-allinone", Predicate::CopiesFrom, "pgrdf"),
            ],
        };
        let report = evaluate(&comp);
        assert!(report.violations.is_empty(), "clean graph: no violations");
        assert!(check_all(&comp).is_ok());
    }

    #[test]
    fn check_all_errs_on_a_hard_violation_but_not_on_warnings() {
        let violating = Composition {
            entities: vec![entity("up", EntityType::UpstreamImage)],
            links: vec![link("up", Predicate::Builds, "x")],
        };
        assert!(check_all(&violating).is_err(), "I1 error blocks emit");

        let warn_only = Composition {
            entities: vec![entity("pure-proc", EntityType::Process)],
            links: vec![link("pure-proc", Predicate::SmokesBy, "none")],
        };
        assert!(
            check_all(&warn_only).is_ok(),
            "a pure warning does not block"
        );
    }

    #[test]
    fn violation_renders_in_the_spec_line_format() {
        let v = Violation {
            code: "I1",
            severity: Severity::Error,
            subject: Some("postgres-bookworm".into()),
            predicate: Some(Predicate::Builds),
            object: Some("patched".into()),
            reason: "never compile an upstream image".into(),
        };
        assert_eq!(
            v.to_string(),
            "I1 postgres-bookworm BUILDS patched: never compile an upstream image"
        );
    }
}
