//! Discipline as graph constraints (SPEC.SPORAXIS §5 — I1–I8).
//!
//! The assembler refuses to emit any bundle that violates a hard invariant. Each
//! one replaces a prose rule a reviewer used to hold in their head:
//!
//! - I1  no `oci:UpstreamImage` is the subject of `BUILDS`   (never compile upstream)   [error]
//! - I2  no `oci:UpstreamImage` is the subject of `SHIMS_FOR` (never patch upstream)     [error]
//! - I3  every `SHIMS_FOR` carries a `notify:` (the NOTIFY it is tracked by)             [error]
//! - I4  every `SHIMS_FOR` carries a `retire_when:` probe + behaviour                    [error]
//! - I5  every `svc:Process` has a `SMOKES_BY` (`… none` ⇒ classified `pure`)            [error/warn]
//! - I6  duplicate-pull: COPIES_FROM(x, upstream) already inherited, unless `reason:`    [warn]
//! - I8  no committed link metadata references a `.gitignore`'d `_WIP/` path             [error]
//!
//! All graph-level. Only **I7** remains — manifest labels derive only from the
//! `oci:FleetImage` entity — and it is an emit-time rule enforced by the renderer
//! (M2), not a pre-emit graph check. I3/I4/I8 read the link YAML *body*, parsed
//! into `ontology::LinkMeta`.

use std::collections::{BTreeMap, BTreeSet};

use crate::ontology::{Composition, EntityType, LinkMeta, Predicate};

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

/// Recursively collect every string scalar inside a YAML value.
fn collect_strings(v: &serde_yaml::Value, out: &mut Vec<String>) {
    match v {
        serde_yaml::Value::String(s) => out.push(s.clone()),
        serde_yaml::Value::Sequence(seq) => seq.iter().for_each(|x| collect_strings(x, out)),
        serde_yaml::Value::Mapping(m) => m.iter().for_each(|(_, val)| collect_strings(val, out)),
        _ => {}
    }
}

/// Does any string in this link's metadata reference a gitignored `_WIP/` path? (I8)
fn references_wip(meta: &LinkMeta) -> bool {
    let mut strings: Vec<String> = [&meta.notify, &meta.reason, &meta.because]
        .into_iter()
        .flatten()
        .cloned()
        .collect();
    if let Some(rw) = &meta.retire_when {
        collect_strings(rw, &mut strings);
    }
    for v in meta.rest.values() {
        collect_strings(v, &mut strings);
    }
    strings.iter().any(|s| s.contains("_WIP/"))
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

        // I3 — every SHIMS_FOR carries the NOTIFY that tracks this shim.
        if l.predicate == Predicate::ShimsFor
            && l.meta
                .notify
                .as_deref()
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        {
            report.violations.push(Violation {
                code: "I3",
                severity: Severity::Error,
                subject: Some(l.subject.clone()),
                predicate: Some(l.predicate),
                object: Some(l.object.clone()),
                reason: "SHIMS_FOR must carry a non-empty `notify:` \
                         (the NOTIFY this shim is tracked by)"
                    .into(),
            });
        }

        // I4 — every SHIMS_FOR carries a retire-when probe + behaviour.
        if l.predicate == Predicate::ShimsFor
            && matches!(l.meta.retire_when, None | Some(serde_yaml::Value::Null))
        {
            report.violations.push(Violation {
                code: "I4",
                severity: Severity::Error,
                subject: Some(l.subject.clone()),
                predicate: Some(l.predicate),
                object: Some(l.object.clone()),
                reason: "SHIMS_FOR must carry a `retire_when:` block \
                         (the probe + the behaviour on probe success)"
                    .into(),
            });
        }

        // I8 — no committed link metadata references a gitignored `_WIP/` path.
        if references_wip(&l.meta) {
            report.violations.push(Violation {
                code: "I8",
                severity: Severity::Error,
                subject: Some(l.subject.clone()),
                predicate: Some(l.predicate),
                object: Some(l.object.clone()),
                reason: "link metadata references a gitignored `_WIP/` path \
                         (use the public form)"
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
        if already_inherited && l.meta.reason.is_none() {
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
    use crate::ontology::{Entity, Link, LinkMeta};

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
            meta: Default::default(),
        }
    }

    fn link_meta(subject: &str, predicate: Predicate, object: &str, meta: LinkMeta) -> Link {
        Link {
            subject: subject.into(),
            predicate,
            object: object.into(),
            meta,
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

    #[test]
    fn the_ck_allinone_example_passes_every_invariant() {
        // The bundled M1 reference composition must stay clean — no errors, no
        // warnings — so it remains a faithful, validatable input for M2.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("examples/ck-allinone.composition");
        let comp = crate::ontology::Composition::load(&dir).expect("load ck-allinone");
        let report = evaluate(&comp);
        assert!(
            !report.has_errors(),
            "ck-allinone must pass: {:?}",
            report.errors().collect::<Vec<_>>()
        );
        assert_eq!(report.warnings().count(), 0, "no warnings expected");
    }

    fn shim(meta: LinkMeta) -> Composition {
        Composition {
            entities: vec![
                entity("relay", EntityType::StaticArtifact),
                entity("pgck", EntityType::DbExtension),
            ],
            links: vec![link_meta("relay", Predicate::ShimsFor, "pgck", meta)],
        }
    }

    #[test]
    fn i3_flags_a_shims_for_edge_without_a_notify() {
        // retire_when present so I4 doesn't also fire — isolate I3.
        let meta = LinkMeta {
            retire_when: Some(serde_yaml::Value::String("probe".into())),
            ..Default::default()
        };
        assert_eq!(
            evaluate(&shim(meta))
                .errors()
                .filter(|v| v.code == "I3")
                .count(),
            1
        );
    }

    #[test]
    fn i3_passes_a_shims_for_with_a_notify() {
        let meta = LinkMeta {
            notify: Some("pgck/feature".into()),
            retire_when: Some(serde_yaml::Value::String("probe".into())),
            ..Default::default()
        };
        assert_eq!(
            evaluate(&shim(meta))
                .errors()
                .filter(|v| v.code == "I3")
                .count(),
            0
        );
    }

    #[test]
    fn i4_flags_a_shims_for_without_a_retire_when() {
        // notify present so I3 doesn't also fire — isolate I4.
        let meta = LinkMeta {
            notify: Some("pgck/feature".into()),
            ..Default::default()
        };
        assert_eq!(
            evaluate(&shim(meta))
                .errors()
                .filter(|v| v.code == "I4")
                .count(),
            1
        );
    }

    #[test]
    fn i8_flags_a_link_field_referencing_a_gitignored_wip_path() {
        // notify + retire_when present (no I3/I4); notify points at a _WIP path.
        let meta = LinkMeta {
            notify: Some("_WIP/NOTIFIES.secret.md".into()),
            retire_when: Some(serde_yaml::Value::String("probe".into())),
            ..Default::default()
        };
        assert_eq!(
            evaluate(&shim(meta))
                .errors()
                .filter(|v| v.code == "I8")
                .count(),
            1
        );
    }

    #[test]
    fn i6_is_exempted_when_the_copies_from_carries_a_reason() {
        let dup = LinkMeta {
            reason: Some("psql trimmed from the base; pulled deliberately".into()),
            ..Default::default()
        };
        let comp = Composition {
            entities: vec![
                entity("app", EntityType::FleetImage),
                entity("mid", EntityType::FleetImage),
                entity("base", EntityType::UpstreamImage),
            ],
            links: vec![
                link("app", Predicate::InheritsFrom, "mid"),
                link("mid", Predicate::InheritsFrom, "base"),
                link_meta("app", Predicate::CopiesFrom, "base", dup),
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
}
