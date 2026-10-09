//! The AP242 semantic PMI writer (design: Architecture items 5 and 6, the Mapping table, the
//! Editions rule and the Anti-requirements table of `docs/step-ap242.md`).
//!
//! [`write`] maps the [`PartPmi`] of any number of parts of one file to one [`Edit`] of its
//! [`Document`]: [`Mode::Add`] writes the items beside what the parts already carry,
//! [`Mode::Replace`] removes what the reader consumed for each part (the removal plan of
//! `removal.rs`, seeded with the part's [`Provenance`](super::Provenance)) and writes the new
//! items, so that afterwards `read(part) == new`; [`Mode::Remove`] is a replace with nothing.
//! Every untouched byte of the file is kept ([`Document::apply`]).
//!
//! **Typed emission.** Records are made only through the emitter below: measures only through
//! [`Emitter::length_measure`] and [`Emitter::angle_measure`] (and the measure representation
//! items built on them), which take a [`LengthUnitRef`] or [`AngleUnitRef`] found per value —
//! a unit instance of the document equal to the value's own unit, or one added — so a value
//! is written in its own unit with its own digits and nothing is converted; a value whose text
//! is not a Part 21 REAL is refused, never rewritten. There is no function taking an untyped
//! number. Complex instances are made only through [`p21::Complex`] (leaves sorted), and only
//! when the set of types is not one entity's supertype closure (the internal mapping is used
//! otherwise). `tolerance_value` is built only from deviations ([`Bounds`]: signed offsets,
//! `upper > lower`), `limits_and_fits` only from an [`Iso286Class`] (two enums). Attributes
//! are placed by name through the schema table, so an instance cannot have too few or too many.
//!
//! **What is refused** (every refusal of every part at once, [`WriteError::Refused`], naming
//! the item): datum targets and the datums they establish, tolerance relations (read, written
//! later), notes (decision 5: the 'manufacturing requirement' text route is not a standard
//! form), general tolerance tables and the part's default decimal places (decision 3: read
//! only), material density (decision 2), groups of unstated kind, dimensions without a nominal
//! value, tolerances on a shape aspect relationship, projected zones without their end, the
//! affected plane, thread and knurl parameter sets their WHERE rules reject, and values that
//! are not Part 21 REALs.
//!
//! **Datum feature symbols** (decision 6): each datum feature of an added datum gets a minimal
//! symbol, derived from the model when written (`PartPlan::datum_symbols`): a tessellated
//! callout in an annotation plane of the part's draughting model, linked to the datum feature by
//! a `draughting_model_item_association`; replace and remove take it with its datum. The model is
//! not related to the part's shape representation (maintainer decision, 2026-10-09).
//!
//! **Usages** (maintainer decision, 2026-10-09): one `geometric_item_specific_usage` per face;
//! a feature of several faces is composed of one member shape aspect per face
//! (`PartPlan::usages`). Part notes in words are 'semantic text' attribute sets (PMI practice
//! §7.4); surface texture is ISO 10303-1110's form (`PartPlan::surface_texture`).
//!
//! **Validation before returning.** The edit is applied; every added or replaced instance must
//! pass `express` validation against the table of the file's edition (the target edition for
//! an AP242 edition without a table, and for an AP214/AP203 file that gains PMI, whose every
//! instance must then pass); and `express_rules::check_all` of the result (the WHERE, UNIQUE
//! and global rules it evaluates, `restrict_representation_for_surface_condition` among them)
//! must have no violation the original lacked. Violations the original already had are
//! reported. The plan's own refusals that cite a rule (thread WR1 and WR12, turned_knurl WR2)
//! are checks of the model before anything is emitted, naming every offending item at once
//! (`Refusal`); `express_rules` stays the check of what was written.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::model::*;
use super::standards::Iso2768;
use super::{Finding, FindingKind, PmiRead, ReadError};
use crate::express::{self, Edition, Violation};
use crate::express_rules::{self, RuleViolation};
use crate::p21::{self, Attribute, Complex, Document, Edit, NewId, P21Error, RawEntity, Record};
use crate::removal::{self, RemovalRefusal};
use crate::step::PartDefinition;

pub use crate::removal::PresentationPolicy;

type A = Attribute;

/// What [`write`] does with each named part's existing PMI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Write the items beside the part's existing PMI. A datum label the part already has
    /// resolves to its existing datum (the same faces), or is refused (other faces).
    Add,
    /// Remove what the reader consumed for the part and write the items: afterwards
    /// `read(part)` equals them. Unconsumed PMI of the part is kept and reported.
    Replace,
    /// Replace with nothing; each part's [`PartPmi`] must be empty.
    Remove,
}

/// An item of a part's PMI, by its index in the [`PartPmi`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemRef {
    Standard(usize),
    DecimalPlaces,
    Feature(usize),
    DatumTarget(usize),
    Datum(usize),
    Dimension(usize),
    Tolerance(usize),
    ToleranceRelation(usize),
    General(usize),
    Thread(usize),
    Knurl(usize),
    Material,
    Note(usize),
    SurfaceTexture(usize),
    Attribute(usize),
}

/// One item the writer will not write, and why.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Refusal {
    pub part: PartId,
    pub item: ItemRef,
    pub why: String,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "part {} {:?}: {}", self.part.0, self.item, self.why)
    }
}

/// Why nothing was written.
#[derive(Clone, Debug, PartialEq)]
pub enum WriteError {
    /// The parts named are not the document's, are named twice, or (remove) carry PMI.
    Parts(String),
    /// A part's PMI violates the model's invariants.
    Invalid {
        part: PartId,
        errors: Vec<ModelError>,
    },
    /// Items the writer does not write (all of them, every part).
    Refused(Vec<Refusal>),
    /// The file's PMI could not be read (replace and add need it).
    Read(ReadError),
    /// A replace or remove is blocked: kept instances reference what it removes.
    Removal(Box<RemovalRefusal>),
    /// The file's schema cannot carry the PMI: an AP214/AP203 file gaining PMI whose instances
    /// do not all validate against the target AP242 edition (decision 1).
    Edition {
        file_schema: String,
        violations: Vec<Violation>,
    },
    /// Instances the writer made that the schema rejects (the backstop; a writer defect).
    Schema(Vec<Violation>),
    /// Rule violations the edit introduces (the backstop).
    Rules(Vec<RuleViolation>),
    /// Part 21 could not be written.
    P21(P21Error),
    /// The writer's own invariant failed (a defect), described.
    Internal(String),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list = |f: &mut std::fmt::Formatter<'_>, v: &[String]| -> std::fmt::Result {
            for (i, s) in v.iter().enumerate() {
                write!(f, "{}{s}", if i == 0 { ": " } else { "; " })?;
            }
            Ok(())
        };
        match self {
            WriteError::Parts(s) => write!(f, "parts: {s}"),
            WriteError::Invalid { part, errors } => {
                write!(f, "part {} PMI is not valid", part.0)?;
                list(f, &errors.iter().map(|e| e.0.clone()).collect::<Vec<_>>())
            }
            WriteError::Refused(r) => {
                write!(f, "not written")?;
                list(f, &r.iter().map(ToString::to_string).collect::<Vec<_>>())
            }
            WriteError::Read(e) => write!(f, "{e}"),
            WriteError::Removal(r) => write!(f, "{r}"),
            WriteError::Edition {
                file_schema,
                violations,
            } => {
                write!(
                    f,
                    "{file_schema} cannot become {}: instances violate it",
                    express::TARGET_SCHEMA
                )?;
                list(
                    f,
                    &violations
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>(),
                )
            }
            WriteError::Schema(v) => {
                write!(f, "written instances violate the schema")?;
                list(f, &v.iter().map(ToString::to_string).collect::<Vec<_>>())
            }
            WriteError::Rules(v) => {
                write!(f, "the edit introduces rule violations")?;
                list(f, &v.iter().map(ToString::to_string).collect::<Vec<_>>())
            }
            WriteError::P21(e) => write!(f, "{e}"),
            WriteError::Internal(s) => write!(f, "writer defect: {s}"),
        }
    }
}

impl std::error::Error for WriteError {}

impl From<P21Error> for WriteError {
    fn from(e: P21Error) -> Self {
        WriteError::P21(e)
    }
}

/// What happened to the file's `FILE_SCHEMA`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SchemaChange {
    /// Kept as it was.
    Kept(Vec<String>),
    /// An AP214/AP203 file that gained PMI, now [`express::TARGET_SCHEMA`] (decision 1).
    Upgraded { from: Vec<String>, to: String },
}

/// What a write did.
#[derive(Clone, Debug, PartialEq)]
pub struct WriteReport {
    pub schema: SchemaChange,
    /// The `FILE_SCHEMA` of the table the written instances were validated against.
    pub validated_against: &'static str,
    /// Instances added.
    pub added: usize,
    /// Instances removed (replace, remove), ascending.
    pub removed: Vec<u64>,
    /// Kept instances rewritten without removed members (presentation containers).
    pub rewritten: Vec<u64>,
    /// Presentation and validation properties removed with the replaced PMI.
    pub presentation_removed: Vec<(u64, String)>,
    /// Shared instances only removed ones referenced, kept unreferenced.
    pub left_unreferenced: Vec<(u64, String)>,
    /// Datums of [`Mode::Add`] that resolved to the part's existing datum: part, label, `#N`.
    pub reused_datums: Vec<(PartId, String, u64)>,
    /// Supplemental geometry items of the file used again rather than written twice.
    pub reused_geometry: Vec<u64>,
    /// Datum feature symbols written (decision 6): part and label(s), in order.
    pub datum_symbols: Vec<(PartId, String)>,
    /// The file's `FILE_SCHEMA` when its AP242 edition has no table: the written instances were
    /// validated against [`WriteReport::validated_against`] (the target edition), so whether
    /// they are valid instances of the file's own edition is undetermined.
    pub edition_undetermined: Option<String>,
    /// The reader's findings about PMI of a replaced part that stays (it was not consumed).
    pub kept_unconsumed: Vec<Finding>,
    /// Schema violations of instances the edit did not make (the original's).
    pub preexisting_violations: Vec<Violation>,
    /// Rule violations the original already had and still has.
    pub preexisting_rule_violations: Vec<RuleViolation>,
}

/// Writes `pmi` (one entry per part, any order; each part at most once) into `doc`, whose
/// `parts` are `read_part_definitions` of the same bytes. One [`Edit`] for all parts, made in
/// part order; deterministic.
pub fn write(
    doc: &Document,
    parts: &[PartDefinition],
    pmi: &[(PartId, PartPmi)],
    mode: Mode,
    policy: PresentationPolicy,
) -> Result<(Edit, WriteReport), WriteError> {
    let mut order: Vec<&(PartId, PartPmi)> = pmi.iter().collect();
    order.sort_by_key(|(p, _)| *p);
    for w in order.windows(2) {
        if w[0].0 == w[1].0 {
            return Err(WriteError::Parts(format!("part {} named twice", w[0].0.0)));
        }
    }
    for (p, part_pmi) in &order {
        if p.0 >= parts.len() {
            return Err(WriteError::Parts(format!(
                "part {} does not exist ({} parts)",
                p.0,
                parts.len()
            )));
        }
        if mode == Mode::Remove && part_pmi != &PartPmi::default() {
            return Err(WriteError::Parts(format!(
                "remove names part {} with PMI to write",
                p.0
            )));
        }
        part_pmi
            .validate()
            .map_err(|errors| WriteError::Invalid { part: *p, errors })?;
    }
    let read = super::read(doc, parts).map_err(WriteError::Read)?;

    // The file's edition and the table written instances are checked against.
    let schema = doc.file_schema()?;
    let first = schema.first().cloned().unwrap_or_default();
    let ap242 = first
        .trim()
        .to_ascii_uppercase()
        .starts_with("AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF");
    let edition: &'static Edition = if ap242 {
        express::edition(&first).unwrap_or(express::TARGET)
    } else {
        express::TARGET
    };

    // Every refusal of every part, before anything is made.
    let mut refusals = Vec::new();
    let mut plans = Vec::new();
    for (p, part_pmi) in &order {
        let plan = PartPlan::new(doc, &parts[p.0], *p, part_pmi, &read, mode)?;
        refusals.extend(plan.refusals.iter().cloned());
        plans.push(plan);
    }
    if !refusals.is_empty() {
        refusals.sort();
        return Err(WriteError::Refused(refusals));
    }

    let mut edit = Edit::new();
    let mut report = WriteReport {
        schema: SchemaChange::Kept(schema.clone()),
        validated_against: edition.file_schema,
        added: 0,
        removed: Vec::new(),
        rewritten: Vec::new(),
        presentation_removed: Vec::new(),
        left_unreferenced: Vec::new(),
        reused_datums: Vec::new(),
        reused_geometry: Vec::new(),
        datum_symbols: Vec::new(),
        edition_undetermined: (ap242 && express::edition(&first).is_none()).then(|| first.clone()),
        kept_unconsumed: Vec::new(),
        preexisting_violations: Vec::new(),
        preexisting_rule_violations: Vec::new(),
    };

    // Replace and remove: one removal plan for every named part's consumed instances.
    let mut removed: BTreeSet<u64> = BTreeSet::new();
    if mode != Mode::Add {
        let mut seeds = BTreeSet::new();
        for (p, _) in &order {
            let prov = &read.provenance.parts[p.0];
            // Supplemental geometry is the part's geometry, not PMI: it is kept (and used
            // again by what is written, matched by value), never removed.
            let geometry: BTreeSet<u64> = prov.geometry.iter().flatten().copied().collect();
            seeds.extend(prov.all().into_iter().filter(|id| !geometry.contains(id)));
        }
        if !seeds.is_empty() {
            // A presentation model (draughting model, view) the removal would empty is the
            // caller's decision (removal.rs): under RemovePresentation the writer removes it
            // with the presentation it held, and lists it; under Refuse it is refused.
            let mut emptied: Vec<(u64, String)> = Vec::new();
            let plan = loop {
                match removal::plan(doc, &seeds, policy) {
                    Ok(plan) => break plan,
                    Err(r)
                        if policy == PresentationPolicy::RemovePresentation
                            && !r.emptied.is_empty()
                            && r.blockers.is_empty()
                            && r.missing.is_empty()
                            && r.infrastructure.is_empty()
                            && r.anchored.is_empty()
                            && r.emptied.iter().all(|e| !seeds.contains(&e.id)) =>
                    {
                        for e in &r.emptied {
                            seeds.insert(e.id);
                            emptied.push((e.id, e.entity.clone()));
                        }
                    }
                    Err(r) if r.emptied.is_empty() && !r.blockers.is_empty() => {
                        // Parts of a removed item's construct the reader does not list: its
                        // `id_attribute`s (§5.1 Figure 4) and edition 4 `uuid_attribute`s and, under RemovePresentation, the
                        // geometric validation properties of it (with their representation
                        // links). Anything else blocks.
                        let extra: Vec<u64> = r
                            .blockers
                            .iter()
                            .map(|b| b.id)
                            .filter(|&b| {
                                is_a(doc, b, "id_attribute")
                                    || is_a(doc, b, "uuid_attribute")
                                    || (policy == PresentationPolicy::RemovePresentation
                                        && validation_property(doc, b, &seeds))
                            })
                            .collect();
                        if extra.len() != r.blockers.len() {
                            return Err(WriteError::Removal(Box::new(r)));
                        }
                        for b in extra {
                            if is_a(doc, b, "property_definition") {
                                emptied.push((b, "property_definition".into()));
                            }
                            seeds.insert(b);
                        }
                    }
                    Err(r) => return Err(WriteError::Removal(Box::new(r))),
                }
            };
            plan.into_edit(&mut edit);
            removed = plan.removed.clone();
            report.removed = plan.removed.iter().copied().collect();
            report.rewritten = plan.rewritten.keys().copied().collect();
            report.presentation_removed = plan.presentation_removed.clone();
            report.presentation_removed.extend(emptied);
            report.presentation_removed.sort();
            report.left_unreferenced = plan.report.left_unreferenced.clone();
        }
        for (p, _) in &order {
            for f in &read.findings {
                if f.part == Some(*p)
                    && matches!(
                        f.kind,
                        FindingKind::Unconsumed
                            | FindingKind::Unsupported
                            | FindingKind::Unresolved
                            | FindingKind::Nonconformance
                    )
                    && f.ids.first().is_some_and(|id| !removed.contains(id))
                {
                    report.kept_unconsumed.push(f.clone());
                }
            }
        }
    }

    // Emission, part by part.
    let mut em = Emitter::new(doc, edition, &mut edit, &removed);
    for plan in &plans {
        plan.emit(&mut em, &mut report)?;
    }
    let added = em.count;
    drop(em);
    report.added = added;

    // Decision 1: an AP214/AP203 file that gains PMI becomes AP242, if it can.
    let upgrade = added > 0 && !ap242;
    if upgrade {
        edit.set_file_schema(vec![express::TARGET_SCHEMA.to_string()]);
        report.schema = SchemaChange::Upgraded {
            from: schema.clone(),
            to: express::TARGET_SCHEMA.to_string(),
        };
    }

    // The backstop: apply, validate what was made, compare the rules.
    let written = doc.apply(&edit)?;
    let after = Document::parse(written.bytes)?;
    let made: BTreeSet<u64> = written
        .ids
        .values()
        .copied()
        .chain(report.rewritten.iter().copied())
        .collect();
    let violations = edition.validate_document(after.graph());
    if upgrade && !violations.is_empty() {
        return Err(WriteError::Edition {
            file_schema: first,
            violations,
        });
    }
    // A rewritten container keeps the violations it had (its other items are the file's).
    let rewritten: BTreeSet<u64> = report.rewritten.iter().copied().collect();
    let had: BTreeSet<(u64, String)> = if rewritten.is_empty() {
        BTreeSet::new()
    } else {
        edition
            .validate_document(doc.graph())
            .into_iter()
            .filter(|v| rewritten.contains(&v.id))
            .map(|v| (v.id, violation_key(&v)))
            .collect()
    };
    let (bad, old): (Vec<Violation>, Vec<Violation>) = violations
        .into_iter()
        .partition(|v| made.contains(&v.id) && !had.contains(&(v.id, violation_key(v))));
    if !bad.is_empty() {
        return Err(WriteError::Schema(bad));
    }
    report.preexisting_violations = old;
    let before: BTreeSet<(u64, &'static str)> = express_rules::check_all(doc)
        .into_iter()
        .map(|v| (v.id, v.rule))
        .collect();
    let (new, kept): (Vec<RuleViolation>, Vec<RuleViolation>) = express_rules::check_all(&after)
        .into_iter()
        .partition(|v| !before.contains(&(v.id, v.rule)));
    if !new.is_empty() {
        return Err(WriteError::Rules(new));
    }
    report.preexisting_rule_violations = kept;
    Ok((edit, report))
}

/// A violation without the positions of list elements (which a rewrite shifts).
fn violation_key(v: &Violation) -> String {
    let mut out = String::new();
    let mut chars = v.detail.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '[' && chars.peek().is_some_and(char::is_ascii_digit) {
            while chars.next().is_some_and(|x| x != ']') {}
            out.push_str("[]");
        } else {
            out.push(c);
        }
    }
    format!("{:?} {:?} {out}", v.kind, v.attribute)
}

// ---------------------------------------------------------------------------------------------
// Instance access (the document's own records)
// ---------------------------------------------------------------------------------------------

fn leaf_names(doc: &Document, id: u64) -> Vec<String> {
    match doc.get(id) {
        Some(RawEntity::Simple { name, .. }) => vec![name.to_ascii_lowercase()],
        Some(RawEntity::Complex { parts, .. }) => {
            parts.iter().map(|p| p.name.to_ascii_lowercase()).collect()
        }
        None => Vec::new(),
    }
}

fn is_a(doc: &Document, id: u64, ancestor: &str) -> bool {
    leaf_names(doc, id).iter().any(|n| {
        express::TARGET.is_a(n, ancestor)
            || (!express::TARGET.is_declared(n) && express::AP242_ED1.is_a(n, ancestor))
    })
}

/// Attribute `attr` declared by `entity` of instance `id`.
fn attr<'d>(doc: &'d Document, id: u64, entity: &str, attr: &str) -> Option<&'d Attribute> {
    match doc.get(id)? {
        RawEntity::Simple {
            name, attributes, ..
        } => {
            let slots = express::TARGET
                .explicit_attributes(name)
                .or_else(|| express::AP242_ED1.explicit_attributes(name))?;
            let i = slots
                .iter()
                .position(|s| s.entity == entity && s.name == attr)?;
            attributes.get(i)
        }
        RawEntity::Complex { parts, .. } => {
            let leaf = parts.iter().find(|p| p.name.eq_ignore_ascii_case(entity))?;
            let decl = express::TARGET
                .entity(entity)
                .or_else(|| express::AP242_ED1.entity(entity))?;
            let i = decl.attributes.iter().position(|a| a.name == attr)?;
            leaf.attributes.get(i)
        }
    }
}

fn attr_ref(doc: &Document, id: u64, entity: &str, a: &str) -> Option<u64> {
    match attr(doc, id, entity, a)? {
        Attribute::EntityRef(n) => Some(*n),
        _ => None,
    }
}

fn attr_refs(doc: &Document, id: u64, entity: &str, a: &str) -> Vec<u64> {
    let mut out = Vec::new();
    if let Some(v) = attr(doc, id, entity, a) {
        p21::visit_attributes(&p21::simple("X", vec![v.clone()]), &mut |x| {
            if let Attribute::EntityRef(n) = x {
                out.push(*n);
            }
        });
    }
    out
}

fn attr_str(doc: &Document, id: u64, entity: &str, a: &str) -> Option<String> {
    match attr(doc, id, entity, a)? {
        Attribute::String(s) => Some(p21::decode(s).unwrap_or_else(|_| s.clone())),
        _ => None,
    }
}

fn attr_enum<'d>(doc: &'d Document, id: u64, entity: &str, a: &str) -> Option<&'d str> {
    match attr(doc, id, entity, a)? {
        Attribute::Enum(s) => Some(s),
        _ => None,
    }
}

/// The shape representation (not a draughting model) that holds a supplemental geometry item,
/// the one a usage of it names (`shape_model`).
fn geometry_rep(doc: &Document, item: u64) -> Option<u64> {
    using_representations(doc, item)
        .into_iter()
        .find(|&r| is_a(doc, r, "shape_representation") && !is_a(doc, r, "draughting_model"))
}

/// A geometric validation property (or its representation link) of something removed: the
/// CAx-IF validation properties, which some files name freely ('length of #4654') but
/// describe as such.
fn validation_property(doc: &Document, id: u64, seeds: &BTreeSet<u64>) -> bool {
    let is_validation = |pd: u64| {
        is_a(doc, pd, "property_definition")
            && (attr_str(doc, pd, "property_definition", "description")
                .is_some_and(|d| d.ends_with("validation property"))
                || attr_str(doc, pd, "property_definition", "name")
                    .is_some_and(|n| express::VALIDATION_PROPERTY_NAMES.contains(&n.as_str())))
    };
    if is_a(doc, id, "property_definition_representation") {
        return attr_ref(doc, id, "property_definition_representation", "definition")
            .is_some_and(|pd| seeds.contains(&pd) && is_validation(pd));
    }
    is_validation(id)
        && attr_ref(doc, id, "property_definition", "definition")
            .is_some_and(|d| seeds.contains(&d))
}

/// The representations whose items hold `item`, directly or through representation items and
/// founded items (the schema's `using_representations`), ascending.
fn using_representations(doc: &Document, item: u64) -> Vec<u64> {
    let mut seen = BTreeSet::from([item]);
    let mut work = vec![item];
    while let Some(x) = work.pop() {
        for &r in doc.referrers(x) {
            if (is_a(doc, r, "representation_item") || is_a(doc, r, "founded_item"))
                && seen.insert(r)
            {
                work.push(r);
            }
        }
    }
    let mut out = BTreeSet::new();
    for &x in &seen {
        for &r in doc.referrers(x) {
            if is_a(doc, r, "representation")
                && attr_refs(doc, r, "representation", "items").contains(&x)
            {
                out.insert(r);
            }
        }
    }
    out.into_iter().collect()
}

/// A length or angle unit of the document, as the reader would read it (`si_unit` with its
/// prefix; `conversion_based_unit` by its factor).
#[derive(Clone, Debug, PartialEq)]
enum DocUnit {
    Length(LengthUnit),
    Angle(AngleUnit),
}

fn doc_unit(doc: &Document, id: u64, depth: usize) -> Option<DocUnit> {
    if depth > 4 {
        return None;
    }
    let names = leaf_names(doc, id);
    let has = |n: &str| names.iter().any(|x| x == n);
    if has("si_unit") {
        let prefix = attr_enum(doc, id, "si_unit", "prefix").map(str::to_ascii_uppercase);
        let name = attr_enum(doc, id, "si_unit", "name")?.to_ascii_uppercase();
        let power = match prefix.as_deref() {
            None => 0,
            Some(p) => si_prefix(p)?,
        };
        return match name.as_str() {
            "METRE" => {
                let full = format!(
                    "{}{}",
                    prefix
                        .as_deref()
                        .map_or(String::new(), str::to_ascii_lowercase),
                    "metre"
                );
                Some(DocUnit::Length(LengthUnit::from_metres(
                    &full,
                    Decimal::parse("1.").ok()?.scaled(power),
                )))
            }
            "RADIAN" if power == 0 => Some(DocUnit::Angle(AngleUnit::Radian)),
            _ => None,
        };
    }
    if has("conversion_based_unit") {
        let name = attr_str(doc, id, "conversion_based_unit", "name")?;
        let f = attr_ref(doc, id, "conversion_based_unit", "conversion_factor")?;
        let (value, unit) = measure_of(doc, f, depth + 1)?;
        return match unit {
            DocUnit::Length(u) => {
                let unit = LengthUnit::from_metres(&name, value.times(&u.metres()));
                Some(DocUnit::Length(unit))
            }
            DocUnit::Angle(a) => {
                let radians = value.to_f64() * a.radians();
                let degree = std::f64::consts::PI / 180.0;
                if (radians / degree - 1.0).abs() < 1e-9 {
                    Some(DocUnit::Angle(AngleUnit::Degree))
                } else if a == AngleUnit::Radian {
                    Some(DocUnit::Angle(AngleUnit::Other {
                        name,
                        radians: value,
                    }))
                } else {
                    None
                }
            }
        };
    }
    None
}

/// The value (as the file writes it) and unit of a measure with unit.
fn measure_of(doc: &Document, id: u64, depth: usize) -> Option<(Decimal, DocUnit)> {
    let v = attr(doc, id, "measure_with_unit", "value_component")?;
    let inner = match v {
        Attribute::Typed { value, .. } => &**value,
        v => v,
    };
    let text = match inner {
        Attribute::Real(r) => p21::encode_real(*r).ok()?,
        Attribute::Integer(i) => format!("{i}."),
        _ => return None,
    };
    let u = attr_ref(doc, id, "measure_with_unit", "unit_component")?;
    Some((Decimal::parse(&text).ok()?, doc_unit(doc, u, depth)?))
}

/// The name the reader gives a unit that is neither a length nor an angle (`Unit::Other`).
fn other_unit_name(doc: &Document, id: u64) -> Option<String> {
    let names = leaf_names(doc, id);
    let has = |n: &str| names.iter().any(|x| x == n);
    if has("si_unit") {
        let prefix = attr_enum(doc, id, "si_unit", "prefix").map(str::to_ascii_lowercase);
        let name = attr_enum(doc, id, "si_unit", "name")?.to_ascii_lowercase();
        if name == "metre" || name == "radian" {
            return None;
        }
        return Some(format!("{}{name}", prefix.unwrap_or_default()));
    }
    if has("conversion_based_unit") {
        if doc_unit(doc, id, 0).is_some() {
            return None;
        }
        return attr_str(doc, id, "conversion_based_unit", "name");
    }
    if has("context_dependent_unit") {
        return attr_str(doc, id, "context_dependent_unit", "name");
    }
    if has("derived_unit") || has("named_unit") {
        return Some(names.join("+"));
    }
    None
}

fn si_prefix(p: &str) -> Option<i64> {
    Some(match p {
        "EXA" => 18,
        "PETA" => 15,
        "TERA" => 12,
        "GIGA" => 9,
        "MEGA" => 6,
        "KILO" => 3,
        "HECTO" => 2,
        "DECA" => 1,
        "DECI" => -1,
        "CENTI" => -2,
        "MILLI" => -3,
        "MICRO" => -6,
        "NANO" => -9,
        "PICO" => -12,
        "FEMTO" => -15,
        "ATTO" => -18,
        _ => return None,
    })
}

// ---------------------------------------------------------------------------------------------
// The typed emission layer
// ---------------------------------------------------------------------------------------------

/// An instance a record references: one of the file's, or one the edit adds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InstanceRef {
    Old(u64),
    New(NewId),
}

impl InstanceRef {
    fn a(self) -> A {
        match self {
            InstanceRef::Old(n) => A::EntityRef(n),
            InstanceRef::New(n) => n.to_ref(),
        }
    }
}

type R = InstanceRef;

/// A length unit instance, found or added for one value's unit: the only unit a
/// `length_measure_with_unit` can be made with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LengthUnitRef(R);

/// A plane angle unit instance, likewise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AngleUnitRef(R);

fn s(text: &str) -> A {
    A::String(p21::escape(text))
}

fn en(v: &str) -> A {
    A::Enum(v.to_ascii_uppercase())
}

fn boolean(v: bool) -> A {
    A::Enum(if v { "T" } else { "F" }.into())
}

fn typed(t: &str, v: A) -> A {
    A::Typed {
        type_name: t.to_ascii_uppercase(),
        value: Box::new(v),
    }
}

fn refs(v: impl IntoIterator<Item = R>) -> A {
    A::List(v.into_iter().map(R::a).collect())
}

/// A stated decimal as a REAL attribute, refused unless it is a Part 21 REAL (it is written
/// with its own text, never rewritten).
fn real(d: &Decimal) -> Result<A, String> {
    p21::encode_real_text(d.as_str()).map_err(|_| {
        format!("value {d} is not a Part 21 REAL (a decimal point is required); not rewritten")
    })?;
    Ok(A::Real(d.to_f64()))
}

/// The emitter: the only way the writer makes records.
pub struct Emitter<'a> {
    doc: &'a Document,
    edition: &'static Edition,
    edit: &'a mut Edit,
    /// Instances the edit removes: never referenced by what it adds.
    removed: &'a BTreeSet<u64>,
    /// Units found or added, by their unit.
    length_units: Vec<(LengthUnit, LengthUnitRef)>,
    angle_units: Vec<(AngleUnit, AngleUnitRef)>,
    other_units: BTreeMap<String, R>,
    exponents: BTreeMap<&'static str, R>,
    count: usize,
}

impl<'a> Emitter<'a> {
    fn new(
        doc: &'a Document,
        edition: &'static Edition,
        edit: &'a mut Edit,
        removed: &'a BTreeSet<u64>,
    ) -> Self {
        Emitter {
            doc,
            edition,
            edit,
            removed,
            length_units: Vec::new(),
            angle_units: Vec::new(),
            other_units: BTreeMap::new(),
            exponents: BTreeMap::new(),
            count: 0,
        }
    }

    /// The provisional reference of the next instance this emitter adds (`#2^63 + n`, as
    /// `p21` documents), for a record that references itself.
    fn next_ref(&self) -> A {
        A::EntityRef((1u64 << 63) | self.count as u64)
    }

    fn push(&mut self, e: RawEntity, stated: &[&Decimal]) -> Result<R, WriteError> {
        let mut rec = Record::new(e);
        let mut seen = BTreeSet::new();
        for d in stated {
            if seen.insert(d.as_str()) {
                rec = rec.state(d.as_str())?;
            }
        }
        let id = self.edit.add(rec);
        if id.index() != self.count {
            return Err(WriteError::Internal(format!(
                "addition {} numbered {}",
                self.count,
                id.index()
            )));
        }
        self.count += 1;
        Ok(R::New(id))
    }

    /// The attribute values of one entity's own or inherited slots, in order, from `vals`
    /// keyed by `(declaring entity, attribute)` (or `("", attribute)` for any entity):
    /// unset when not given and optional, derived when redeclared so, refused when missing.
    fn place(
        entity: &str,
        slots: &[(&'static str, &'static str, bool, bool)],
        vals: &[((&str, &str), A)],
    ) -> Result<Vec<A>, WriteError> {
        let mut used = vec![false; vals.len()];
        let mut out = Vec::new();
        for &(decl, name, optional, derived) in slots {
            if derived {
                out.push(A::Derived);
                continue;
            }
            let found = vals.iter().enumerate().position(|(i, ((e, n), _))| {
                !used[i] && *n == name && (e.is_empty() || *e == decl)
            });
            match found {
                Some(i) => {
                    used[i] = true;
                    out.push(vals[i].1.clone());
                }
                None if optional => out.push(A::Unset),
                None => {
                    return Err(WriteError::Internal(format!(
                        "{entity}: no value for {decl}.{name}"
                    )));
                }
            }
        }
        if let Some(i) = used.iter().position(|u| !u) {
            return Err(WriteError::Internal(format!(
                "{entity}: {:?} is not one of its attributes",
                vals[i].0
            )));
        }
        Ok(out)
    }

    /// An instance of the types `types` (each with all its supertypes implied): a simple record
    /// when one of them is a subtype of all the others (internal mapping), otherwise a complex
    /// record of the supertype closure (external mapping, leaves sorted by [`Complex`]).
    fn instance(
        &mut self,
        types: &[&str],
        vals: &[((&str, &str), A)],
        stated: &[&Decimal],
    ) -> Result<R, WriteError> {
        let ed = self.edition;
        let closure = |n: &str| -> BTreeSet<&'static str> {
            let mut out = BTreeSet::new();
            let mut stack: Vec<&'static str> = ed.entity(n).map(|e| e.name).into_iter().collect();
            while let Some(x) = stack.pop() {
                if out.insert(x)
                    && let Some(e) = ed.entity(x)
                {
                    stack.extend(e.supertypes.iter().copied());
                }
            }
            out
        };
        for t in types {
            if ed.entity(t).is_none() {
                return Err(WriteError::Internal(format!(
                    "{t} is not an entity of {}",
                    ed.file_schema
                )));
            }
        }
        let all: BTreeSet<&'static str> = types.iter().flat_map(|t| closure(t)).collect();
        if let Some(top) = types.iter().find(|t| closure(t) == all) {
            let slots: Vec<_> = ed
                .explicit_attributes(top)
                .unwrap_or_default()
                .into_iter()
                .map(|s| (s.entity, s.name, s.optional, s.derived))
                .collect();
            let attributes = Self::place(top, &slots, vals)?;
            return self.push(p21::simple(&top.to_ascii_uppercase(), attributes), stated);
        }
        // Complex: each leaf its own attributes; one redeclared as derived by any leaf is `*`.
        let mut leaves = Vec::new();
        let mut remaining: Vec<((&str, &str), A)> = vals.to_vec();
        for leaf in &all {
            let decl = ed.entity(leaf).expect("in the table");
            let slots: Vec<_> = decl
                .attributes
                .iter()
                .map(|a| {
                    let derived = all.iter().filter_map(|x| ed.entity(x)).any(|x| {
                        x.redeclared
                            .iter()
                            .any(|r| r.entity == *leaf && r.attribute == a.name && r.derived)
                    });
                    (decl.name, a.name, a.optional, derived)
                })
                .collect();
            let mine: Vec<((&str, &str), A)> = remaining
                .iter()
                .filter(|((e, n), _)| {
                    (*e == *leaf || e.is_empty()) && decl.attributes.iter().any(|a| a.name == *n)
                })
                .cloned()
                .collect();
            remaining.retain(|((e, n), _)| {
                !((*e == *leaf || e.is_empty()) && decl.attributes.iter().any(|a| a.name == *n))
            });
            leaves.push(p21::leaf(
                &leaf.to_ascii_uppercase(),
                Self::place(leaf, &slots, &mine)?,
            ));
        }
        if let Some(((e, n), _)) = remaining.first() {
            return Err(WriteError::Internal(format!(
                "{types:?}: {e}.{n} is not one of its attributes"
            )));
        }
        let complex = Complex::new(leaves)?;
        self.push(complex.into_record(), stated)
    }

    /// A simple instance of `entity`, attributes by name.
    fn simple(&mut self, entity: &str, vals: &[(&str, A)]) -> Result<R, WriteError> {
        let v: Vec<((&str, &str), A)> = vals.iter().map(|(n, a)| (("", *n), a.clone())).collect();
        self.instance(&[entity], &v, &[])
    }

    fn reference(&self, id: u64) -> Result<R, WriteError> {
        if self.removed.contains(&id) {
            return Err(WriteError::Internal(format!(
                "#{id} is removed by the edit but referenced by what it adds"
            )));
        }
        Ok(R::Old(id))
    }

    // --- units -----------------------------------------------------------------------------

    fn dimensional_exponents(&mut self, kind: &'static str) -> Result<R, WriteError> {
        if let Some(r) = self.exponents.get(kind) {
            return Ok(*r);
        }
        let length = if kind == "length" { 1.0 } else { 0.0 };
        let r = self.simple(
            "dimensional_exponents",
            &[
                ("length_exponent", A::Real(length)),
                ("mass_exponent", A::Real(0.0)),
                ("time_exponent", A::Real(0.0)),
                ("electric_current_exponent", A::Real(0.0)),
                ("thermodynamic_temperature_exponent", A::Real(0.0)),
                ("amount_of_substance_exponent", A::Real(0.0)),
                ("luminous_intensity_exponent", A::Real(0.0)),
            ],
        )?;
        self.exponents.insert(kind, r);
        Ok(r)
    }

    /// A unit instance equal to `unit`: one of `context`'s units, else one added (SI with its
    /// prefix, or a conversion based unit with its exact factor).
    pub fn length_unit(
        &mut self,
        context: u64,
        unit: &LengthUnit,
    ) -> Result<LengthUnitRef, WriteError> {
        if let Some((_, r)) = self.length_units.iter().find(|(u, _)| same_length(u, unit)) {
            return Ok(*r);
        }
        let found = attr_refs(self.doc, context, "global_unit_assigned_context", "units")
            .into_iter()
            .find(|&u| {
                matches!(doc_unit(self.doc, u, 0), Some(DocUnit::Length(l)) if same_length(&l, unit))
            });
        let r = match found {
            Some(u) => LengthUnitRef(self.reference(u)?),
            None => {
                let si = |prefix: Option<&str>| {
                    vec![
                        (("si_unit", "prefix"), prefix.map_or(A::Unset, en)),
                        (("si_unit", "name"), en("metre")),
                    ]
                };
                let r = match unit {
                    LengthUnit::Millimetre => {
                        self.instance(&["length_unit", "si_unit"], &si(Some("milli")), &[])?
                    }
                    LengthUnit::Micrometre => {
                        self.instance(&["length_unit", "si_unit"], &si(Some("micro")), &[])?
                    }
                    LengthUnit::Centimetre => {
                        self.instance(&["length_unit", "si_unit"], &si(Some("centi")), &[])?
                    }
                    LengthUnit::Metre => {
                        self.instance(&["length_unit", "si_unit"], &si(None), &[])?
                    }
                    LengthUnit::Inch | LengthUnit::Foot | LengthUnit::Other { .. } => {
                        let (name, factor, base) = match unit {
                            LengthUnit::Inch => {
                                ("INCH".to_string(), "25.4", LengthUnit::Millimetre)
                            }
                            LengthUnit::Foot => {
                                ("FOOT".to_string(), "304.8", LengthUnit::Millimetre)
                            }
                            LengthUnit::Other { name, metres } => {
                                (name.clone(), metres.as_str(), LengthUnit::Metre)
                            }
                            _ => unreachable!("matched above"),
                        };
                        let factor =
                            Decimal::parse(factor).map_err(|e| WriteError::Internal(e.0))?;
                        let base_unit = base;
                        let base = self.length_unit(context, &base_unit)?;
                        let f = self.length_measure(
                            &Length {
                                value: factor,
                                unit: base_unit,
                            },
                            base,
                        )?;
                        let dims = self.dimensional_exponents("length")?;
                        self.instance(
                            &["conversion_based_unit", "length_unit"],
                            &[
                                (("named_unit", "dimensions"), dims.a()),
                                (("conversion_based_unit", "name"), s(&name)),
                                (("conversion_based_unit", "conversion_factor"), f.a()),
                            ],
                            &[],
                        )?
                    }
                };
                LengthUnitRef(r)
            }
        };
        self.length_units.push((unit.clone(), r));
        Ok(r)
    }

    /// A plane angle unit instance equal to `unit`, found in `context` or added.
    pub fn angle_unit(
        &mut self,
        context: u64,
        unit: &AngleUnit,
    ) -> Result<AngleUnitRef, WriteError> {
        if let Some((_, r)) = self.angle_units.iter().find(|(u, _)| same_angle(u, unit)) {
            return Ok(*r);
        }
        let found = attr_refs(self.doc, context, "global_unit_assigned_context", "units")
            .into_iter()
            .find(|&u| {
                matches!(doc_unit(self.doc, u, 0), Some(DocUnit::Angle(a)) if same_angle(&a, unit))
            });
        let r = match found {
            Some(u) => AngleUnitRef(self.reference(u)?),
            None => {
                let r = match unit {
                    AngleUnit::Radian => self.instance(
                        &["plane_angle_unit", "si_unit"],
                        &[
                            (("si_unit", "prefix"), A::Unset),
                            (("si_unit", "name"), en("radian")),
                        ],
                        &[],
                    )?,
                    AngleUnit::Degree | AngleUnit::Other { .. } => {
                        let (name, factor) = match unit {
                            AngleUnit::Degree => (
                                "DEGREE".to_string(),
                                Decimal::parse(&p21::encode_real(std::f64::consts::PI / 180.0)?)
                                    .map_err(|e| WriteError::Internal(e.0))?,
                            ),
                            AngleUnit::Other { name, radians } => (name.clone(), radians.clone()),
                            AngleUnit::Radian => unreachable!("matched above"),
                        };
                        let base = self.angle_unit(context, &AngleUnit::Radian)?;
                        let f = self.angle_measure(
                            &Angle {
                                value: factor,
                                unit: AngleUnit::Radian,
                            },
                            base,
                        )?;
                        let dims = self.dimensional_exponents("none")?;
                        self.instance(
                            &["conversion_based_unit", "plane_angle_unit"],
                            &[
                                (("named_unit", "dimensions"), dims.a()),
                                (("conversion_based_unit", "name"), s(&name)),
                                (("conversion_based_unit", "conversion_factor"), f.a()),
                            ],
                            &[],
                        )?
                    }
                };
                AngleUnitRef(r)
            }
        };
        self.angle_units.push((unit.clone(), r));
        Ok(r)
    }

    /// A dimensionless unit: a ratio unit of the context, else one added.
    fn ratio_unit(&mut self, context: u64) -> Result<R, WriteError> {
        if let Some(r) = self.other_units.get("ratio") {
            return Ok(*r);
        }
        let found = attr_refs(self.doc, context, "global_unit_assigned_context", "units")
            .into_iter()
            .find(|&u| is_a(self.doc, u, "ratio_unit"));
        let r = match found {
            Some(u) => self.reference(u)?,
            None => {
                // A ratio unit with a name of its own (context dependent): the reader resolves
                // a named unit by its leaves, and a lone RATIO_UNIT's internal mapping names
                // none it knows.
                let dims = self.dimensional_exponents("none")?;
                self.instance(
                    &["context_dependent_unit", "ratio_unit"],
                    &[
                        (("named_unit", "dimensions"), dims.a()),
                        (("context_dependent_unit", "name"), s("ratio")),
                    ],
                    &[],
                )?
            }
        };
        self.other_units.insert("ratio".into(), r);
        Ok(r)
    }

    /// The unit of a count (a context dependent unit 'count': ISO 10303-41 has no count unit).
    fn count_unit(&mut self) -> Result<R, WriteError> {
        if let Some(r) = self.other_units.get("count") {
            return Ok(*r);
        }
        let dims = self.dimensional_exponents("none")?;
        let r = self.instance(
            &["context_dependent_unit"],
            &[
                (("named_unit", "dimensions"), dims.a()),
                (("context_dependent_unit", "name"), s("count")),
            ],
            &[],
        )?;
        self.other_units.insert("count".into(), r);
        Ok(r)
    }

    // --- measures --------------------------------------------------------------------------

    /// `LENGTH_MEASURE_WITH_UNIT(LENGTH_MEASURE(v), #u)`, `v` written as stated.
    pub fn length_measure(&mut self, l: &Length, unit: LengthUnitRef) -> Result<R, WriteError> {
        let v = real(&l.value).map_err(WriteError::Internal)?;
        self.instance(
            &["length_measure_with_unit"],
            &[
                (("", "value_component"), typed("length_measure", v)),
                (("", "unit_component"), unit.0.a()),
            ],
            &[&l.value],
        )
    }

    /// `PLANE_ANGLE_MEASURE_WITH_UNIT(PLANE_ANGLE_MEASURE(v), #u)`, `v` written as stated.
    pub fn angle_measure(&mut self, a: &Angle, unit: AngleUnitRef) -> Result<R, WriteError> {
        let v = real(&a.value).map_err(WriteError::Internal)?;
        self.instance(
            &["plane_angle_measure_with_unit"],
            &[
                (("", "value_component"), typed("plane_angle_measure", v)),
                (("", "unit_component"), unit.0.a()),
            ],
            &[&a.value],
        )
    }

    /// A value as a measure with unit, its §5.4 decimal places by a `measure_qualification`.
    fn value(&mut self, context: u64, v: &Value) -> Result<R, WriteError> {
        let m = match &v.quantity {
            Quantity::Length(l) => {
                let u = self.length_unit(context, &l.unit)?;
                self.length_measure(l, u)?
            }
            Quantity::Angle(a) => {
                let u = self.angle_unit(context, &a.unit)?;
                self.angle_measure(a, u)?
            }
        };
        if let Some(p) = v.decimal_places {
            let q = self.format_qualifier(v.decimal(), p)?;
            self.simple(
                "measure_qualification",
                &[
                    ("name", s("")),
                    ("description", s("")),
                    ("qualified_measure", m.a()),
                    ("qualifiers", refs([q])),
                ],
            )?;
        }
        Ok(m)
    }

    /// `VALUE_FORMAT_TYPE_QUALIFIER('NR2 x.y')` (§5.4): `x` the value's integer digits, `y`
    /// its decimal places.
    fn format_qualifier(&mut self, d: &Decimal, places: u8) -> Result<R, WriteError> {
        let int = d
            .as_str()
            .trim_start_matches(['+', '-'])
            .split(['.', 'E', 'e'])
            .next()
            .unwrap_or("")
            .trim_start_matches('0')
            .len()
            .max(1);
        self.simple(
            "value_format_type_qualifier",
            &[("format_type", s(&format!("NR2 {int}.{places}")))],
        )
    }

    /// A named measure representation item of a length, angle, ratio or count, with
    /// qualifiers: the typed leaf (`LENGTH_MEASURE_WITH_UNIT`, …) is part of the instance.
    fn measure_item(
        &mut self,
        name: &str,
        m: MeasureItem<'_>,
        context: u64,
        mut qualifiers: Vec<R>,
        places: Option<u8>,
    ) -> Result<R, WriteError> {
        let (leaf, value_type, decimal, unit) = match m {
            MeasureItem::Length(l) => (
                Some("length_measure_with_unit"),
                "length_measure",
                &l.value,
                self.length_unit(context, &l.unit)?.0,
            ),
            MeasureItem::Angle(a) => (
                Some("plane_angle_measure_with_unit"),
                "plane_angle_measure",
                &a.value,
                self.angle_unit(context, &a.unit)?.0,
            ),
            MeasureItem::Ratio(r) => (
                Some("ratio_measure_with_unit"),
                "ratio_measure",
                r,
                self.ratio_unit(context)?,
            ),
            MeasureItem::Count(c) => (None, "count_measure", c, self.count_unit()?),
        };
        if let Some(p) = places {
            qualifiers.push(self.format_qualifier(decimal, p)?);
        }
        let v = real(decimal).map_err(WriteError::Internal)?;
        let mut types = vec!["measure_representation_item"];
        types.extend(leaf);
        if !qualifiers.is_empty() {
            types.push("qualified_representation_item");
        }
        let mut vals = vec![
            (("", "name"), s(name)),
            (("", "value_component"), typed(value_type, v)),
            (("", "unit_component"), unit.a()),
        ];
        if !qualifiers.is_empty() {
            vals.push((("", "qualifiers"), refs(qualifiers)));
        }
        self.instance(&types, &vals, &[decimal])
    }

    /// `TOLERANCE_VALUE(lower_bound, upper_bound)` from deviations: signed offsets from the
    /// nominal, `upper > lower` (the only way a tolerance value is made).
    fn tolerance_value(&mut self, context: u64, deviations: &Bounds) -> Result<R, WriteError> {
        let lower = self.value(context, deviations.lower())?;
        let upper = self.value(context, deviations.upper())?;
        self.simple(
            "tolerance_value",
            &[("lower_bound", lower.a()), ("upper_bound", upper.a())],
        )
    }

    /// `LIMITS_AND_FITS(form_variance, zone_variance, grade, source)` from an ISO 286 class
    /// (§5.2.5): the deviation's letters, 'hole' or 'shaft' by their case, the grade's number.
    fn limits_and_fits(&mut self, class: &Iso286Class) -> Result<R, WriteError> {
        let zone = match class.deviation.of {
            FitFeature::Hole => "hole",
            FitFeature::Shaft => "shaft",
        };
        self.simple(
            "limits_and_fits",
            &[
                ("form_variance", s(&class.deviation.to_string())),
                ("zone_variance", s(zone)),
                ("grade", s(class.grade.number())),
                ("source", s("")),
            ],
        )
    }
}

fn same_length(a: &LengthUnit, b: &LengthUnit) -> bool {
    match (a, b) {
        (LengthUnit::Other { name: n, metres: m }, LengthUnit::Other { name: o, metres: p }) => {
            n == o && m.cmp_value(p).is_eq()
        }
        _ => a == b,
    }
}

fn same_angle(a: &AngleUnit, b: &AngleUnit) -> bool {
    match (a, b) {
        (
            AngleUnit::Other {
                name: n,
                radians: m,
            },
            AngleUnit::Other {
                name: o,
                radians: p,
            },
        ) => n == o && m.cmp_value(p).is_eq(),
        _ => a == b,
    }
}

/// What a measure representation item holds.
#[derive(Clone, Copy)]
enum MeasureItem<'v> {
    Length(&'v Length),
    Angle(&'v Angle),
    Ratio(&'v Decimal),
    Count(&'v Decimal),
}

fn measure_of_value(v: &Value) -> MeasureItem<'_> {
    match &v.quantity {
        Quantity::Length(l) => MeasureItem::Length(l),
        Quantity::Angle(a) => MeasureItem::Angle(a),
    }
}

// ---------------------------------------------------------------------------------------------
// One part: the plan (checks, roles, reuse) and its emission
// ---------------------------------------------------------------------------------------------

/// A feature's role, which decides its entity type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Role {
    Plain,
    DatumFeature,
    /// The datum feature is also the one size dimension on it (§6.5.3).
    DatumFeatureOfSize(usize),
    AppliedArea,
    ThreadRunout,
}

struct PartPlan<'a> {
    doc: &'a Document,
    def: &'a PartDefinition,
    id: PartId,
    pmi: &'a PartPmi,
    mode: Mode,
    refusals: Vec<Refusal>,
    /// The part's shape representation and its context.
    shape_rep: u64,
    context: u64,
    /// Feature → the feature written for it (features of equal content are one instance).
    canonical: Vec<usize>,
    roles: BTreeMap<usize, Role>,
    /// Features that are also an applied area or a thread runout beside their own role: a
    /// second instance of that type.
    twins: BTreeSet<(usize, Role)>,
    /// Features something refers to (other than as an area or a runout).
    referenced: BTreeSet<usize>,
    /// Datums resolved to the part's existing datum (add): datum → (`#datum`, `#datum_feature`).
    existing_datums: BTreeMap<usize, (u64, Option<u64>)>,
    /// Geometry → an existing supplemental geometry item with the same value.
    existing_geometry: BTreeMap<usize, u64>,
    /// Existing datum system names on the part (kept ones).
    system_names: BTreeSet<String>,
}

impl<'a> PartPlan<'a> {
    fn new(
        doc: &'a Document,
        def: &'a PartDefinition,
        id: PartId,
        pmi: &'a PartPmi,
        read: &PmiRead,
        mode: Mode,
    ) -> Result<Self, WriteError> {
        let shape_rep = doc
            .referrers(def.shape)
            .iter()
            .copied()
            .filter(|&r| is_a(doc, r, "shape_definition_representation"))
            .filter_map(|r| {
                attr_ref(
                    doc,
                    r,
                    "property_definition_representation",
                    "used_representation",
                )
            })
            .min()
            .ok_or_else(|| {
                WriteError::Parts(format!(
                    "part {} has no shape representation for #{}",
                    id.0, def.shape
                ))
            })?;
        let context =
            attr_ref(doc, shape_rep, "representation", "context_of_items").ok_or_else(|| {
                WriteError::Parts(format!("shape representation #{shape_rep} has no context"))
            })?;
        let mut plan = PartPlan {
            doc,
            def,
            id,
            pmi,
            mode,
            refusals: Vec::new(),
            shape_rep,
            context,
            canonical: (0..pmi.features.len()).collect(),
            roles: BTreeMap::new(),
            twins: BTreeSet::new(),
            referenced: BTreeSet::new(),
            existing_datums: BTreeMap::new(),
            existing_geometry: BTreeMap::new(),
            system_names: BTreeSet::new(),
        };
        plan.check(read);
        Ok(plan)
    }

    fn refuse(&mut self, item: ItemRef, why: impl Into<String>) {
        self.refusals.push(Refusal {
            part: self.id,
            item,
            why: why.into(),
        });
    }

    fn context_length_unit(&self) -> Option<LengthUnit> {
        attr_refs(
            self.doc,
            self.context,
            "global_unit_assigned_context",
            "units",
        )
        .into_iter()
        .find_map(|u| match doc_unit(self.doc, u, 0) {
            Some(DocUnit::Length(l)) => Some(l),
            _ => None,
        })
    }

    /// Every check that can refuse an item, and the roles and reuse decisions.
    fn check(&mut self, read: &PmiRead) {
        let p = self.pmi;
        let existing = &read.parts[self.id.0];
        let prov = &read.provenance.parts[self.id.0];

        // Values: every decimal must be a Part 21 REAL.
        let bad_value =
            |vals: &[&Decimal]| -> Option<String> { vals.iter().find_map(|d| real(d).err()) };

        for (i, s) in p.standards.iter().enumerate() {
            let key = s.edition.clone().unwrap_or_else(|| s.document.clone());
            let body = if key.starts_with("ASME") || s.document.starts_with("ASME") {
                StandardBody::Asme
            } else if key.starts_with("ISO") || s.document.starts_with("ISO") {
                StandardBody::Iso
            } else {
                StandardBody::Other
            };
            if body != s.body || s.document.trim().is_empty() || s.document.trim() != s.document {
                self.refuse(
                    ItemRef::Standard(i),
                    format!(
                        "standard {:?} ({:?}): the body is what its document or edition names (§4), and the document is a non-empty name",
                        s.document, s.body
                    ),
                );
            }
        }
        if p.decimal_places.is_some() {
            self.refuse(
                ItemRef::DecimalPlaces,
                "the part's default decimal places are held by a default_tolerance_table, which is read only (decision 3)",
            );
        }
        for i in 0..p.datum_targets.len() {
            self.refuse(
                ItemRef::DatumTarget(i),
                "datum targets are read but not written yet (design: Out of scope)",
            );
        }
        for i in 0..p.tolerance_relations.len() {
            self.refuse(
                ItemRef::ToleranceRelation(i),
                "tolerance relations (composite frames) are read but not written yet (design: Out of scope)",
            );
        }
        for i in 0..p.notes.len() {
            self.refuse(
                ItemRef::Note(i),
                "notes are the non-standard 'manufacturing requirement' text route, which is not written (decision 5)",
            );
        }
        for (i, g) in p.general.iter().enumerate() {
            match g {
                GeneralTolerance::Table { .. } => self.refuse(
                    ItemRef::General(i),
                    "a default_tolerance_table is read, not written (decision 3)",
                ),
                GeneralTolerance::Class { text, standard } => {
                    if Iso2768::recognise(text) != *standard || text.trim().is_empty() {
                        self.refuse(
                            ItemRef::General(i),
                            format!("tolerance class {text:?}: its ISO 2768 classes are those its text states"),
                        );
                    }
                }
            }
        }
        if let Some(m) = &p.material {
            if m.density.is_some() {
                self.refuse(
                    ItemRef::Material,
                    "material density is read only (decision 2: no density is written)",
                );
            }
            if m.id.is_empty() {
                self.refuse(ItemRef::Material, "material without its identification");
            }
        }

        // Features: anchors on the part, geometry, groups.
        let ctx_unit = self.context_length_unit();
        let existing_geometry: Vec<(&Geometry, u64)> = existing
            .geometry
            .iter()
            .zip(&prov.geometry_items)
            .map(|(g, &id)| (g, id))
            .collect();
        for (gi, g) in p.geometry.iter().enumerate() {
            if let Some(&(_, id)) = existing_geometry
                .iter()
                .find(|(e, id)| *e == g && geometry_rep(self.doc, *id).is_some())
            {
                self.existing_geometry.insert(gi, id);
            }
        }
        for (i, f) in p.features.iter().enumerate() {
            match f {
                Feature::Items(items) => {
                    for a in items {
                        match *a {
                            Anchor::Face(x) if x.0 >= self.def.faces.len() => self.refuse(
                                ItemRef::Feature(i),
                                format!("face {} is not a face of the part ({} faces)", x.0, self.def.faces.len()),
                            ),
                            Anchor::Edge(x) if x.0 >= self.def.edges.len() => self.refuse(
                                ItemRef::Feature(i),
                                format!("edge {} is not an edge of the part ({} edges)", x.0, self.def.edges.len()),
                            ),
                            Anchor::Geometry(g) if !self.existing_geometry.contains_key(&g.0) => {
                                let geo = &p.geometry[g.0];
                                if geo.length_unit.is_some() && geo.length_unit != ctx_unit {
                                    self.refuse(
                                        ItemRef::Feature(i),
                                        format!(
                                            "supplemental geometry {} is in {:?}, the part's context in {:?}",
                                            g.0, geo.length_unit, ctx_unit
                                        ),
                                    );
                                }
                                if let Some(why) = geometry_values(&geo.item).err() {
                                    self.refuse(ItemRef::Feature(i), why);
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Feature::Group {
                    kind: GroupKind::Unstated,
                    ..
                } => self.refuse(
                    ItemRef::Feature(i),
                    "a group of unstated kind (§6.4 requires 'multiple elements' or 'pattern of features')",
                ),
                _ => {}
            }
        }

        // Features of equal content are one instance (one usage per item and representation,
        // item_identified_representation_usage UR1).
        let mut by_content: BTreeMap<Vec<Anchor>, usize> = BTreeMap::new();
        for (i, f) in p.features.iter().enumerate() {
            if let Feature::Items(items) = f {
                let mut key = items.clone();
                key.sort();
                key.dedup();
                let first = *by_content.entry(key).or_insert(i);
                self.canonical[i] = first;
            }
        }

        // Roles.
        let mut roles: BTreeMap<usize, BTreeSet<Role>> = BTreeMap::new();
        for d in &p.datums {
            if let Some(f) = d.feature() {
                roles
                    .entry(self.canonical[f.0])
                    .or_default()
                    .insert(Role::DatumFeature);
            }
        }
        for (i, t) in p.threads.iter().enumerate() {
            match t.partial_area {
                Some(f) => {
                    roles.entry(self.canonical[f.0]).or_default().insert(Role::AppliedArea);
                }
                None => self.refuse(
                    ItemRef::Thread(i),
                    "a thread needs its 'partial area occurrence' feature (thread WR12: exactly one)",
                ),
            }
            if let Some(f) = t.runout {
                roles
                    .entry(self.canonical[f.0])
                    .or_default()
                    .insert(Role::ThreadRunout);
            }
        }
        for k in &p.knurls {
            roles
                .entry(self.canonical[k.feature.0])
                .or_default()
                .insert(Role::AppliedArea);
        }
        let twin_role = |r: &Role| matches!(r, Role::AppliedArea | Role::ThreadRunout);
        for (f, rs) in &roles {
            // A datum on a group whose members it applies to individually is the group and
            // the datum feature in one complex instance (§6.5.2); no other group or derived
            // feature takes a role.
            let group_datum = rs.len() == 1
                && rs.contains(&Role::DatumFeature)
                && matches!(
                    p.features[*f],
                    Feature::Group {
                        kind: GroupKind::MultipleElements | GroupKind::PatternOfFeatures,
                        ..
                    }
                );
            if !matches!(p.features[*f], Feature::Items(_)) && !group_datum {
                self.refuse(
                    ItemRef::Feature(*f),
                    format!("a group or derived feature used as {rs:?}"),
                );
            }
        }
        // Features something refers to other than as a thread's or knurl's area or a thread's
        // runout.
        let mut referenced: BTreeSet<usize> = BTreeSet::new();
        {
            let mut add = |f: FeatureId| {
                referenced.insert(self.canonical[f.0]);
            };
            for d in &p.datums {
                d.feature().into_iter().for_each(&mut add);
            }
            for d in &p.dimensions {
                match &d.kind {
                    DimensionKind::Size { feature, path, .. } => {
                        add(*feature);
                        path.iter().copied().for_each(&mut add);
                    }
                    DimensionKind::Location { from, to, path, .. } => {
                        add(*from);
                        add(*to);
                        path.iter().copied().for_each(&mut add);
                    }
                }
            }
            for t in &p.tolerances {
                if let ToleranceTarget::Feature(f) = t.target {
                    add(f);
                }
                if let Some(z) = &t.zone {
                    z.projected.iter().filter_map(|x| x.end).for_each(&mut add);
                    z.non_uniform.iter().flatten().copied().for_each(&mut add);
                }
            }
            for f in &p.features {
                if let Feature::Group { members, .. } | Feature::Derived { from: members, .. } = f {
                    members.iter().copied().for_each(&mut add);
                }
            }
            for th in &p.threads {
                add(th.feature);
            }
            for k in &p.knurls {
                add(k.feature);
            }
            for a in &p.attributes {
                if let Some(NoteOwner::Feature(f)) = a.on {
                    add(f);
                }
            }
            for s in &p.surface_textures {
                s.on.into_iter().for_each(&mut add);
            }
        }
        for (f, rs) in roles {
            // A thread's or knurl's area and a thread's runout are shape aspects of their own
            // types (applied_area, thread_runout; thread WR12, WR16). Where the same faces are
            // also a feature in their own right (the threaded faces, a datum feature), that type
            // is a second instance composed of the feature (§6.5.2, UR1), so that what states
            // the feature keeps the plain shape aspect or datum feature.
            let primary = rs.iter().find(|r| !twin_role(r)).copied();
            let mut twins: Vec<Role> = rs.iter().filter(|r| twin_role(r)).copied().collect();
            let primary = match primary {
                Some(r) => r,
                None if referenced.contains(&f) => Role::Plain,
                None => twins.remove(0),
            };
            self.roles.insert(f, primary);
            for r in twins {
                self.twins.insert((f, r));
            }
        }
        self.referenced = referenced;
        // A datum feature with exactly one size dimension on it, nothing else stated of it
        // as a feature, is that dimension (§6.5.3).
        let datum_features: Vec<usize> = self
            .roles
            .iter()
            .filter(|(_, r)| **r == Role::DatumFeature)
            .map(|(f, _)| *f)
            .collect();
        for f in datum_features {
            let sizes: Vec<usize> = p
                .dimensions
                .iter()
                .enumerate()
                .filter(|(_, d)| matches!(&d.kind, DimensionKind::Size { feature, path: None, angle: None, .. } if self.canonical[feature.0] == f))
                .map(|(i, _)| i)
                .collect();
            let stated_as_feature = p.tolerances.iter().any(
                |t| matches!(t.target, ToleranceTarget::Feature(x) if self.canonical[x.0] == f),
            ) || p
                .attributes
                .iter()
                .any(|a| matches!(a.on, Some(NoteOwner::Feature(x)) if self.canonical[x.0] == f))
                || p.surface_textures
                    .iter()
                    .any(|s| matches!(s.on, Some(x) if self.canonical[x.0] == f));
            if let [d] = sizes.as_slice()
                && !stated_as_feature
            {
                self.roles.insert(f, Role::DatumFeatureOfSize(*d));
            }
        }

        // Datums: one per label per part; add resolves an existing label.
        for (i, d) in p.datums.iter().enumerate() {
            if !d.targets().is_empty() {
                self.refuse(
                    ItemRef::Datum(i),
                    format!(
                        "datum {} is established by datum targets, which are not written yet",
                        d.label()
                    ),
                );
                continue;
            }
            if self.mode != Mode::Add {
                continue;
            }
            let Some(e) = existing.datums.iter().position(|x| x.label() == d.label()) else {
                continue;
            };
            let old = &existing.datums[e];
            let same = match (old.feature(), d.feature()) {
                (Some(a), Some(b)) => {
                    let set = |f: &Feature| match f {
                        Feature::Items(items) => {
                            let mut v = items.clone();
                            v.sort();
                            v.dedup();
                            Some(v)
                        }
                        _ => None,
                    };
                    old.targets().is_empty()
                        && set(&existing.features[a.0]).is_some()
                        && set(&existing.features[a.0]) == set(&p.features[b.0])
                }
                _ => false,
            };
            if !same {
                self.refuse(
                    ItemRef::Datum(i),
                    format!("the part already has datum {} on other faces (one datum per label per part)", d.label()),
                );
                continue;
            }
            let datum = prov.datums[e]
                .iter()
                .copied()
                .find(|&x| is_a(self.doc, x, "datum"));
            let feature = datum.and_then(|dx| {
                self.doc.referrers(dx).iter().copied().find_map(|r| {
                    (is_a(self.doc, r, "shape_aspect_relationship")
                        && attr_ref(
                            self.doc,
                            r,
                            "shape_aspect_relationship",
                            "related_shape_aspect",
                        ) == Some(dx))
                    .then(|| {
                        attr_ref(
                            self.doc,
                            r,
                            "shape_aspect_relationship",
                            "relating_shape_aspect",
                        )
                    })
                    .flatten()
                    .filter(|&f| is_a(self.doc, f, "datum_feature"))
                })
            });
            match (datum, feature) {
                (Some(dx), Some(fx)) => {
                    self.existing_datums.insert(i, (dx, Some(fx)));
                }
                _ => self.refuse(
                    ItemRef::Datum(i),
                    format!(
                        "the part's datum {} could not be resolved to its instances",
                        d.label()
                    ),
                ),
            }
        }
        if self.mode == Mode::Add {
            for &r in self.doc.referrers(self.def.shape) {
                if is_a(self.doc, r, "datum_system")
                    && let Some(n) = attr_str(self.doc, r, "shape_aspect", "name")
                {
                    self.system_names.insert(n);
                }
            }
        }

        // Dimensions.
        for (i, d) in p.dimensions.iter().enumerate() {
            if d.nominal.is_none() {
                self.refuse(
                    ItemRef::Dimension(i),
                    "a dimension without a nominal value (§5.2.1: it shall always be given)",
                );
            }
            let mut vals: Vec<&Decimal> = d.nominal.iter().map(Value::decimal).collect();
            match &d.tolerance {
                DimTolerance::Deviations(b) | DimTolerance::Limits(b) => {
                    vals.extend([b.upper().decimal(), b.lower().decimal()]);
                }
                DimTolerance::Fit {
                    limits: Some(b), ..
                } => {
                    vals.extend([b.upper().decimal(), b.lower().decimal()]);
                }
                _ => {}
            }
            if let Some(why) = bad_value(&vals) {
                self.refuse(ItemRef::Dimension(i), why);
            }
        }

        // Tolerances.
        for (i, t) in p.tolerances.iter().enumerate() {
            if matches!(t.target, ToleranceTarget::Relation { .. }) {
                self.refuse(
                    ItemRef::Tolerance(i),
                    "a tolerance on a shape_aspect_relationship is not written: the reader reads such a relationship from a feature as the feature's composition",
                );
            }
            if let Some(z) = &t.zone {
                if z.projected.as_ref().is_some_and(|pz| pz.end.is_none()) {
                    self.refuse(
                        ItemRef::Tolerance(i),
                        "a projected tolerance zone without its projection end (§6.9.2.2)",
                    );
                }
                if z.affected_plane.is_some() {
                    self.refuse(
                        ItemRef::Tolerance(i),
                        "the affected plane (§6.9.2.1) is not written: the reader does not hold it",
                    );
                }
            }
            if let Some(u) = &t.unit_basis {
                let lengths = std::iter::once(&u.size)
                    .chain(u.area.as_ref().and_then(|a| a.second.as_ref()))
                    .all(|v| matches!(v.quantity, Quantity::Length(_)));
                if !lengths {
                    self.refuse(
                        ItemRef::Tolerance(i),
                        "a unit basis that is not a length (geometric_tolerance_with_defined_unit.unit_size is a length_measure_with_unit)",
                    );
                }
            }
            let mut vals: Vec<&Decimal> = Vec::new();
            vals.extend(t.magnitude.iter().map(Value::decimal));
            vals.extend(t.maximum.iter().map(Value::decimal));
            vals.extend(t.unequal.iter().map(Value::decimal));
            if let Some(z) = &t.zone {
                vals.extend(z.projected.iter().map(|p| p.length.decimal()));
                vals.extend(z.runout_angle.iter().map(Value::decimal));
            }
            if let Some(why) = bad_value(&vals) {
                self.refuse(ItemRef::Tolerance(i), why);
            }
        }

        // Threads and knurls: their parameter counts (thread WR1, turned_knurl WR2), refused
        // here per item, before emission; express_rules checks the written instances.
        for (i, t) in p.threads.iter().enumerate() {
            let n = 6
                + usize::from(t.minor_diameter.is_some())
                + usize::from(t.pitch_diameter.is_some())
                + usize::from(t.fit_class_2.is_some())
                + usize::from(t.crest.is_some())
                + usize::from(t.qualifier.is_some())
                + usize::from(t.nominal_size.is_some());
            if !(8..=11).contains(&n) {
                self.refuse(
                    ItemRef::Thread(i),
                    format!("a thread with {n} parameters (thread WR1: 8 to 11)"),
                );
            }
            let mut vals = vec![&t.major_diameter.value, &t.number_of_threads.0];
            for l in [
                &t.minor_diameter,
                &t.pitch_diameter,
                &t.crest,
                &t.nominal_size,
            ]
            .into_iter()
            .flatten()
            {
                vals.push(&l.value);
            }
            if let Some(why) = bad_value(&vals) {
                self.refuse(ItemRef::Thread(i), why);
            }
        }
        for (i, k) in p.knurls.iter().enumerate() {
            let n = 3
                + usize::from(k.number_of_teeth.is_some())
                + usize::from(k.tooth_depth.is_some())
                + usize::from(k.root_fillet.is_some())
                + usize::from(k.helix_angle.is_some())
                + usize::from(k.helix_hand.is_some());
            if !(6..=9).contains(&n) {
                self.refuse(
                    ItemRef::Knurl(i),
                    format!("a knurl with {n} parameters (turned_knurl WR2: 6 to 9)"),
                );
            }
            let mut vals = vec![
                &k.major_diameter.value,
                &k.nominal_diameter.value,
                &k.diametral_pitch.value,
            ];
            for l in [&k.tooth_depth, &k.root_fillet].into_iter().flatten() {
                vals.push(&l.value);
            }
            vals.extend(k.helix_angle.iter().map(|a| &a.value));
            if let Some(why) = bad_value(&vals) {
                self.refuse(ItemRef::Knurl(i), why);
            }
        }

        // Attributes.
        for (i, a) in p.attributes.iter().enumerate() {
            if matches!(a.on, Some(NoteOwner::DatumTarget(_))) {
                self.refuse(
                    ItemRef::Attribute(i),
                    "an attribute on a datum target (datum targets are not written yet)",
                );
            }
            for (key, v) in &a.items {
                let why = match v {
                    AttributeValue::Measure(m) if m.decimal_places.is_some() => Some(
                        "decimal places of an attribute value are not held by the UDA form"
                            .to_string(),
                    ),
                    AttributeValue::Measure(m) => real(m.decimal()).err(),
                    AttributeValue::Real(d) => real(d).err(),
                    // The reader looks for the value of a boolean_representation_item as its
                    // own attribute (it is boolean_literal's), so a written boolean would not
                    // read back.
                    AttributeValue::Boolean(_) => Some(
                        "a boolean value is not written: the reader does not read boolean_representation_item.the_value back".to_string(),
                    ),
                    AttributeValue::OtherMeasure { value, unit, .. } => {
                        real(value).err().or_else(|| {
                            (!unit.is_empty() && self.other_unit(unit).is_err())
                                .then(|| self.other_unit(unit).err().unwrap_or_default())
                        })
                    }
                    _ => None,
                };
                if let Some(why) = why {
                    self.refuse(ItemRef::Attribute(i), format!("{key:?}: {why}"));
                }
            }
        }

        // Surface textures.
        for (i, s) in p.surface_textures.iter().enumerate() {
            let vals: Vec<&Decimal> = s.parameters.iter().map(|x| &x.value.value).collect();
            if let Some(why) = bad_value(&vals) {
                self.refuse(ItemRef::SurfaceTexture(i), why);
            }
        }
    }

    /// The document's unit the reader names `name` (an attribute's measure of another kind):
    /// exactly one distinct instance, else refused.
    fn other_unit(&self, name: &str) -> Result<u64, String> {
        let found: Vec<u64> = self
            .doc
            .ids()
            .filter(|&id| is_a(self.doc, id, "named_unit") || is_a(self.doc, id, "derived_unit"))
            .filter(|&id| other_unit_name(self.doc, id).as_deref() == Some(name))
            .collect();
        match found.as_slice() {
            [u] => Ok(*u),
            [] => Err(format!("no unit of the file is {name:?}")),
            _ => Err(format!(
                "{} units of the file are named {name:?}; which one is not stated",
                found.len()
            )),
        }
    }

    /// The representation of the part that holds a face or edge (the B-rep's).
    fn rep_of(&self, item: u64) -> Result<u64, WriteError> {
        using_representations(self.doc, item)
            .into_iter()
            .find(|&r| is_a(self.doc, r, "shape_representation"))
            .ok_or_else(|| WriteError::Internal(format!("#{item} is in no shape representation")))
    }

    // --- emission --------------------------------------------------------------------------

    fn emit(&self, em: &mut Emitter<'_>, report: &mut WriteReport) -> Result<(), WriteError> {
        let mut st = Emitted::default();
        let p = self.pmi;
        let pds = em.reference(self.def.shape)?;
        let pd = em.reference(self.def.product_definition)?;

        for s in &p.standards {
            self.standard(em, pd, s)?;
        }

        // Supplemental geometry not already in the file: its items, then one constructive
        // geometry representation of the part holding them.
        let mut new_items: Vec<(usize, R)> = Vec::new();
        let mut used_geometry: BTreeSet<usize> = BTreeSet::new();
        for f in &p.features {
            if let Feature::Items(items) = f {
                for a in items {
                    if let Anchor::Geometry(g) = a {
                        used_geometry.insert(g.0);
                    }
                }
            }
        }
        let mut memo: HashMap<GeometryItem, R> = HashMap::new();
        for &g in &used_geometry {
            if let Some(&old) = self.existing_geometry.get(&g) {
                st.geometry.insert(g, (em.reference(old)?, None));
                report.reused_geometry.push(old);
            } else {
                let r = geometry_item(em, &p.geometry[g].item, &mut memo)?;
                new_items.push((g, r));
            }
        }
        if !new_items.is_empty() {
            let cgr = em.simple(
                "constructive_geometry_representation",
                &[
                    ("name", s("supplemental geometry")),
                    ("items", refs(new_items.iter().map(|(_, r)| *r))),
                    ("context_of_items", em.reference(self.context)?.a()),
                ],
            )?;
            em.simple(
                "constructive_geometry_representation_relationship",
                &[
                    ("name", s("supplemental geometry")),
                    ("description", s("")),
                    ("rep_1", em.reference(self.shape_rep)?.a()),
                    ("rep_2", cgr.a()),
                ],
            )?;
            for (g, r) in new_items {
                st.geometry.insert(g, (r, Some(cgr)));
            }
        }

        // Features resolved to an existing datum feature write no usage.
        let reused: BTreeSet<usize> = self
            .existing_datums
            .iter()
            .filter(|(_, (_, fx))| fx.is_some())
            .filter_map(|(d, _)| p.datums[*d].feature())
            .map(|f| self.canonical[f.0])
            .collect();
        let mut keys: BTreeMap<(R, Vec<R>), usize> = BTreeMap::new();
        let mut done = BTreeSet::new();
        for (i, f) in p.features.iter().enumerate() {
            let c = self.canonical[i];
            if let Feature::Items(items) = f
                && done.insert(c)
                && !reused.contains(&c)
            {
                for k in self.usage_keys(em, &st, items)? {
                    *keys.entry(k).or_insert(0) += 1;
                }
            }
        }
        for &(c, _) in &self.twins {
            if let Feature::Items(items) = &p.features[c] {
                for k in self.usage_keys(em, &st, items)? {
                    *keys.entry(k).or_insert(0) += 1;
                }
            }
        }
        let shared: BTreeSet<(R, Vec<R>)> = keys
            .iter()
            .filter(|(_, n)| **n > 1)
            .map(|(k, _)| k.clone())
            .collect();
        let mut done = BTreeSet::new();
        for (i, f) in p.features.iter().enumerate() {
            let c = self.canonical[i];
            if let Feature::Items(items) = f
                && done.insert(c)
                && !reused.contains(&c)
                && self.referenced.contains(&c)
                && let [k] = self.usage_keys(em, &st, items)?.as_slice()
                && shared.contains(k)
            {
                st.owners.entry(k.clone()).or_insert(c);
            }
        }
        st.shared_keys = keys
            .into_iter()
            .filter(|(_, n)| *n > 1)
            .map(|(k, _)| k)
            .collect();
        for i in 0..p.features.len() {
            self.feature(em, &mut st, pds, i)?;
        }
        // Decision 6: a datum feature symbol per datum feature of an added datum.
        let mut symbols: Vec<(R, Vec<String>, usize)> = Vec::new();
        for (i, d) in p.datums.iter().enumerate() {
            if let Some(&(dx, _)) = self.existing_datums.get(&i) {
                st.datums.insert(i, em.reference(dx)?);
                report
                    .reused_datums
                    .push((self.id, d.label().as_str().to_string(), dx));
                continue;
            }
            let datum = em.simple(
                "datum",
                &[
                    ("name", s("")),
                    ("description", s("")),
                    ("of_shape", pds.a()),
                    ("product_definitional", boolean(false)),
                    ("identification", s(d.label().as_str())),
                ],
            )?;
            if let Some(f) = d.feature() {
                let fr = self.feature(em, &mut st, pds, f.0)?;
                em.simple(
                    "shape_aspect_relationship",
                    &[
                        ("name", s("")),
                        ("description", s("")),
                        ("relating_shape_aspect", fr.a()),
                        ("related_shape_aspect", datum.a()),
                    ],
                )?;
                match symbols.iter_mut().find(|(r, _, _)| *r == fr) {
                    Some((_, labels, _)) => labels.push(d.label().as_str().to_string()),
                    None => symbols.push((fr, vec![d.label().as_str().to_string()], f.0)),
                }
            }
            st.datums.insert(i, datum);
        }
        self.datum_symbols(em, &symbols, report)?;
        for i in 0..p.dimensions.len() {
            self.dimension(em, &mut st, pds, i)?;
        }
        let mut names = self.system_names.clone();
        for (i, t) in p.tolerances.iter().enumerate() {
            let r = self.tolerance(em, &mut st, pds, t, &mut names)?;
            st.tolerances.insert(i, r);
        }
        for g in &p.general {
            if let GeneralTolerance::Class { text, .. } = g {
                let item = em.simple(
                    "descriptive_representation_item",
                    &[("name", s("tolerance class")), ("description", s(text))],
                )?;
                self.property(
                    em,
                    pds,
                    "default tolerances",
                    "",
                    "default tolerances",
                    &[item],
                )?;
            }
        }
        for t in &p.threads {
            self.thread(em, &mut st, pds, t)?;
        }
        for k in &p.knurls {
            self.knurl(em, &mut st, pds, k)?;
        }
        if let Some(m) = &p.material {
            let item = em.simple(
                "descriptive_representation_item",
                &[
                    ("name", s(&m.id)),
                    ("description", s(m.name.as_deref().unwrap_or(""))),
                ],
            )?;
            self.property(
                em,
                pd,
                "material property",
                "material name",
                "material name",
                &[item],
            )?;
        }
        for a in &p.attributes {
            self.attribute_set(em, &mut st, pd, pds, a)?;
        }
        for t in &p.surface_textures {
            self.surface_texture(em, &mut st, pds, t)?;
        }
        Ok(())
    }

    /// A point of feature `f` in the part's coordinates: of its first face, edge or
    /// supplemental geometry item (a group's or derived feature's first member's).
    fn feature_point(&self, f: usize) -> Option<[f64; 3]> {
        match &self.pmi.features[f] {
            Feature::Items(items) => items.iter().find_map(|a| match *a {
                Anchor::Face(x) => first_point(self.doc, self.def.faces[x.0]),
                Anchor::Edge(x) => first_point(self.doc, self.def.edges[x.0]),
                Anchor::Geometry(g) => geometry_point(&self.pmi.geometry[g.0].item),
            }),
            Feature::Group { members, .. } | Feature::Derived { from: members, .. } => {
                members.iter().find_map(|m| self.feature_point(m.0))
            }
        }
    }

    /// Decision 6: for each datum feature of an added datum, a minimal datum feature symbol
    /// (the label boxed, a stem to a triangle on the feature), derived from the semantics:
    /// one tessellated callout per feature (PMI practice §8.2, the set named for its PMI type,
    /// 'datum'), each in its own annotation plane (§9.1), the planes in one draughting model of
    /// the part related to its shape representation, each callout linked to its datum feature
    /// by a `draughting_model_item_association` ('PMI representation to presentation link',
    /// §7.3). Replace and remove take the symbol with its datum (removal plan, policy
    /// `RemovePresentation`).
    fn datum_symbols(
        &self,
        em: &mut Emitter<'_>,
        symbols: &[(R, Vec<String>, usize)],
        report: &mut WriteReport,
    ) -> Result<(), WriteError> {
        if symbols.is_empty() {
            return Ok(());
        }
        let mut tips = Vec::new();
        for (_, labels, f) in symbols {
            tips.push(self.feature_point(*f).ok_or_else(|| {
                WriteError::Internal(format!(
                    "datum {}: its feature has no point for its symbol",
                    labels.join(",")
                ))
            })?);
        }
        let mut points = vertices(self.doc, &self.def.faces);
        points.extend(tips.iter().copied());
        let mut layout = SymbolLayout::new(&points);
        let context = em.reference(self.context)?;
        let font = em.simple(
            "draughting_pre_defined_curve_font",
            &[("name", s("continuous"))],
        )?;
        let black = em.simple("draughting_pre_defined_colour", &[("name", s("black"))])?;
        let width = (layout.height / 14.0 * 1e6).round() / 1e6;
        let curve = em.simple(
            "curve_style",
            &[
                ("name", s("")),
                ("curve_font", font.a()),
                (
                    "curve_width",
                    typed("positive_length_measure", A::Real(width)),
                ),
                ("curve_colour", black.a()),
            ],
        )?;
        let line_style = em.simple(
            "presentation_style_assignment",
            &[("styles", refs([curve]))],
        )?;
        // The annotation plane's style, as NIST's test files give it.
        let colour = em.simple("colour", &[])?;
        let fill_colour = em.simple(
            "fill_area_style_colour",
            &[("name", s("")), ("fill_colour", colour.a())],
        )?;
        let fill = em.simple(
            "fill_area_style",
            &[("name", s("")), ("fill_styles", refs([fill_colour]))],
        )?;
        let plane_style =
            em.simple("presentation_style_assignment", &[("styles", refs([fill]))])?;
        let point = |em: &mut Emitter<'_>, p: [f64; 3]| {
            em.simple(
                "cartesian_point",
                &[
                    ("name", s("")),
                    ("coordinates", A::List(p.map(A::Real).to_vec())),
                ],
            )
        };
        let direction = |em: &mut Emitter<'_>, d: [f64; 3]| {
            em.simple(
                "direction",
                &[
                    ("name", s("")),
                    ("direction_ratios", A::List(d.map(A::Real).to_vec())),
                ],
            )
        };
        let mut planes = Vec::new();
        let mut callouts = Vec::new();
        for ((feature, labels, _), tip) in symbols.iter().zip(&tips) {
            let label = labels.join(",");
            let name = format!("Datum {label}");
            let ([origin, normal, along], lines) = layout.draw(*tip, &label);
            let coordinates: Vec<A> = lines
                .iter()
                .flatten()
                .map(|p| A::List(p.map(A::Real).to_vec()))
                .collect();
            let mut strips = Vec::new();
            let mut first = 1i64;
            for l in &lines {
                strips.push(A::List(
                    (0..l.len() as i64).map(|k| A::Integer(first + k)).collect(),
                ));
                first += l.len() as i64;
            }
            let list = em.simple(
                "coordinates_list",
                &[
                    ("name", s("")),
                    ("npoints", A::Integer(coordinates.len() as i64)),
                    ("position_coords", A::List(coordinates)),
                ],
            )?;
            let curves = em.simple(
                "tessellated_curve_set",
                &[
                    ("name", s("datum")),
                    ("coordinates", list.a()),
                    ("line_strips", A::List(strips)),
                ],
            )?;
            let set = em.simple(
                "tessellated_geometric_set",
                &[("name", s("datum")), ("children", refs([curves]))],
            )?;
            let occurrence = em.simple(
                "tessellated_annotation_occurrence",
                &[
                    ("name", s(&name)),
                    ("styles", refs([line_style])),
                    ("item", set.a()),
                ],
            )?;
            let callout = em.simple(
                "draughting_callout",
                &[("name", s(&name)), ("contents", refs([occurrence]))],
            )?;
            let (o, n, a) = (
                point(em, origin)?,
                direction(em, normal)?,
                direction(em, along)?,
            );
            let placement = em.simple(
                "axis2_placement_3d",
                &[
                    ("name", s("")),
                    ("location", o.a()),
                    ("axis", n.a()),
                    ("ref_direction", a.a()),
                ],
            )?;
            let plane = em.simple("plane", &[("name", s(&name)), ("position", placement.a())])?;
            let annotation_plane = em.simple(
                "annotation_plane",
                &[
                    ("name", s(&name)),
                    ("styles", refs([plane_style])),
                    ("item", plane.a()),
                    ("elements", refs([callout])),
                ],
            )?;
            planes.push(annotation_plane);
            callouts.push((*feature, callout));
            report.datum_symbols.push((self.id, label));
        }
        let model = em.simple(
            "draughting_model",
            &[
                ("name", s("datum feature symbols")),
                ("items", refs(planes)),
                ("context_of_items", context.a()),
            ],
        )?;
        // No mechanical_design_and_draughting_relationship relates the model to the part's
        // shape representation (maintainer decision, 2026-10-09): specify-core's OpenCascade
        // loader crashed on every file with one (docs/step-ap242.md gives the cause); the
        // associations below link each symbol.
        for (feature, callout) in callouts {
            em.simple(
                "draughting_model_item_association",
                &[
                    ("name", s("PMI representation to presentation link")),
                    ("description", s("")),
                    ("definition", feature.a()),
                    ("used_representation", model.a()),
                    ("identified_item", callout.a()),
                ],
            )?;
        }
        Ok(())
    }

    /// `property_definition(name, description, owner)` with one representation of `items`.
    fn property(
        &self,
        em: &mut Emitter<'_>,
        owner: R,
        name: &str,
        description: &str,
        rep_name: &str,
        items: &[R],
    ) -> Result<R, WriteError> {
        let pdef = em.simple(
            "property_definition",
            &[
                ("name", s(name)),
                ("description", s(description)),
                ("definition", owner.a()),
            ],
        )?;
        let rep = em.simple(
            "representation",
            &[
                ("name", s(rep_name)),
                ("items", refs(items.iter().copied())),
                ("context_of_items", em.reference(self.context)?.a()),
            ],
        )?;
        em.simple(
            "property_definition_representation",
            &[("definition", pdef.a()), ("used_representation", rep.a())],
        )?;
        Ok(pdef)
    }

    fn standard(&self, em: &mut Emitter<'_>, pd: R, s_: &Standard) -> Result<(), WriteError> {
        let app = em.simple(
            "application_context",
            &[(
                "application",
                s("geometrical dimensioning and tolerancing representation"),
            )],
        )?;
        let ctx = em.simple(
            "product_definition_context",
            &[
                ("name", s("")),
                ("frame_of_reference", app.a()),
                ("life_cycle_stage", s("design")),
            ],
        )?;
        let role = em.simple(
            "product_definition_context_role",
            &[("name", s("additional context")), ("description", s(""))],
        )?;
        em.simple(
            "product_definition_context_association",
            &[
                ("definition", pd.a()),
                ("frame_of_reference", ctx.a()),
                ("role", role.a()),
            ],
        )?;
        let kind = em.simple("document_type", &[("product_data_type", s("standard"))])?;
        let doc = em.simple(
            "document",
            &[
                ("id", s(&s_.document)),
                ("name", s("")),
                ("description", s("")),
                ("kind", kind.a()),
            ],
        )?;
        let adr = em.simple(
            "applied_document_reference",
            &[
                ("assigned_document", doc.a()),
                ("source", s("")),
                ("items", refs([ctx])),
            ],
        )?;
        let orole = em.simple(
            "object_role",
            &[("name", s("mandatory")), ("description", s(""))],
        )?;
        em.simple(
            "role_association",
            &[("role", orole.a()), ("item_with_role", adr.a())],
        )?;
        if let Some(edition) = &s_.edition {
            let pmi_app = em.simple(
                "application_context",
                &[("application", s("product and manufacturing information"))],
            )?;
            let pctx = em.simple(
                "product_context",
                &[
                    ("name", s("")),
                    ("frame_of_reference", pmi_app.a()),
                    ("discipline_type", s("mechanical")),
                ],
            )?;
            let product = em.simple(
                "product",
                &[
                    ("id", s(&s_.document)),
                    ("name", s("")),
                    ("description", s("")),
                    ("frame_of_reference", refs([pctx])),
                ],
            )?;
            em.simple(
                "product_related_product_category",
                &[
                    ("name", s("document")),
                    ("description", s("")),
                    ("products", refs([product])),
                ],
            )?;
            let pdf = em.simple(
                "product_definition_formation",
                &[
                    ("id", s(edition)),
                    ("description", s("")),
                    ("of_product", product.a()),
                ],
            )?;
            em.simple(
                "document_product_equivalence",
                &[
                    ("name", s("equivalence")),
                    ("description", s("")),
                    ("relating_document", doc.a()),
                    ("related_product", pdf.a()),
                ],
            )?;
        }
        Ok(())
    }

    /// The instance of feature `i` (memoised; features of equal content share one).
    fn feature(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        i: usize,
    ) -> Result<R, WriteError> {
        let c = self.canonical[i];
        if let Some(&r) = st.features.get(&c) {
            return Ok(r);
        }
        // A datum of add resolved to the part's existing datum: its datum feature.
        for (d, &(_, fx)) in &self.existing_datums {
            if let (Some(f), Some(fx)) = (self.pmi.datums[*d].feature(), fx)
                && self.canonical[f.0] == c
            {
                let r = em.reference(fx)?;
                st.features.insert(c, r);
                return Ok(r);
            }
        }
        let base = |pd: bool| {
            vec![
                (("shape_aspect", "name"), s("")),
                (("shape_aspect", "description"), s("")),
                (("shape_aspect", "of_shape"), pds.a()),
                (("shape_aspect", "product_definitional"), boolean(pd)),
            ]
        };
        let r = match &self.pmi.features[c] {
            Feature::Items(items) => {
                let role = self.roles.get(&c).copied().unwrap_or(Role::Plain);
                let r = match role {
                    Role::Plain => em.instance(&["shape_aspect"], &base(true), &[])?,
                    Role::DatumFeature => em.instance(&["datum_feature"], &base(true), &[])?,
                    Role::AppliedArea => em.instance(&["applied_area"], &base(true), &[])?,
                    Role::ThreadRunout => em.instance(&["thread_runout"], &base(true), &[])?,
                    Role::DatumFeatureOfSize(d) => {
                        let DimensionKind::Size { kind, .. } = &self.pmi.dimensions[d].kind else {
                            return Err(WriteError::Internal(
                                "a size dimension that is not".into(),
                            ));
                        };
                        let me = em.next_ref();
                        let mut vals = base(true);
                        vals.push((("dimensional_size", "applies_to"), me));
                        vals.push((("dimensional_size", "name"), s(kind.name())));
                        let r =
                            em.instance(&["dimensional_size_with_datum_feature"], &vals, &[])?;
                        st.dimensions.insert(d, r);
                        r
                    }
                };
                st.features.insert(c, r);
                self.usages(em, st, pds, r, Some(c), items)?;
                r
            }
            Feature::Group { members, kind } => {
                let (ty, desc) = match kind {
                    GroupKind::MultipleElements => {
                        ("composite_group_shape_aspect", "multiple elements")
                    }
                    GroupKind::PatternOfFeatures => {
                        ("composite_group_shape_aspect", "pattern of features")
                    }
                    GroupKind::AllAround => ("all_around_shape_aspect", ""),
                    GroupKind::Between => ("between_shape_aspect", ""),
                    GroupKind::Unstated => {
                        return Err(WriteError::Internal(
                            "an unstated group passed the checks".into(),
                        ));
                    }
                };
                let mut vals = base(true);
                vals[1].1 = s(desc);
                let mut ms = Vec::new();
                for m in members {
                    ms.push(self.feature(em, st, pds, m.0)?);
                }
                let r = if self.roles.get(&c) == Some(&Role::DatumFeature) {
                    em.instance(&[ty, "datum_feature"], &vals, &[])?
                } else {
                    em.instance(&[ty], &vals, &[])?
                };
                for m in ms {
                    em.simple(
                        "shape_aspect_relationship",
                        &[
                            ("name", s("")),
                            ("description", s("")),
                            ("relating_shape_aspect", r.a()),
                            ("related_shape_aspect", m.a()),
                        ],
                    )?;
                }
                st.features.insert(c, r);
                r
            }
            Feature::Derived { kind, from } => {
                let ty = match kind {
                    DerivedKind::Derived => "derived_shape_aspect",
                    DerivedKind::Apex => "apex",
                    DerivedKind::CentreOfSymmetry => "centre_of_symmetry",
                    DerivedKind::GeometricAlignment => "geometric_alignment",
                    DerivedKind::PerpendicularTo => "perpendicular_to",
                    DerivedKind::Extension => "extension",
                    DerivedKind::Tangent => "tangent",
                    DerivedKind::ParallelOffset => "parallel_offset",
                };
                let mut bases = Vec::new();
                for f in from {
                    bases.push(self.feature(em, st, pds, f.0)?);
                }
                let r = em.instance(&[ty], &base(false), &[])?;
                for b in bases {
                    em.simple(
                        "shape_aspect_deriving_relationship",
                        &[
                            ("name", s("")),
                            ("description", s("")),
                            ("relating_shape_aspect", r.a()),
                            ("related_shape_aspect", b.a()),
                        ],
                    )?;
                }
                st.features.insert(c, r);
                r
            }
        };
        Ok(r)
    }

    /// The instance of feature `f` in `role`: the feature's own, or its second instance of
    /// that type (an applied area or thread runout on faces that are also another feature).
    fn role_instance(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        f: FeatureId,
        role: Role,
    ) -> Result<R, WriteError> {
        let c = self.canonical[f.0];
        if !self.twins.contains(&(c, role)) {
            return self.feature(em, st, pds, f.0);
        }
        if let Some(&r) = st.twins.get(&(c, role)) {
            return Ok(r);
        }
        let Feature::Items(items) = &self.pmi.features[c] else {
            return Err(WriteError::Internal(
                "a twin of a feature that is not items".into(),
            ));
        };
        let ty = if role == Role::AppliedArea {
            "applied_area"
        } else {
            "thread_runout"
        };
        let r = em.simple(
            ty,
            &[
                ("name", s("")),
                ("description", s("")),
                ("of_shape", pds.a()),
                ("product_definitional", boolean(true)),
            ],
        )?;
        st.twins.insert((c, role), r);
        self.usages(em, st, pds, r, None, items)?;
        Ok(r)
    }

    /// The (representation, item) keys of a feature's usages: one per item, in model order
    /// (one `geometric_item_specific_usage` per face, maintainer decision 2026-10-09).
    fn usage_keys(
        &self,
        em: &Emitter<'_>,
        st: &Emitted,
        items: &[Anchor],
    ) -> Result<Vec<(R, Vec<R>)>, WriteError> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for a in items {
            if !seen.insert(*a) {
                continue;
            }
            let (item, rep) = match *a {
                Anchor::Face(f) => {
                    let id = self.def.faces[f.0];
                    (em.reference(id)?, em.reference(self.rep_of(id)?)?)
                }
                Anchor::Edge(e) => {
                    let id = self.def.edges[e.0];
                    (em.reference(id)?, em.reference(self.rep_of(id)?)?)
                }
                Anchor::Geometry(g) => {
                    let (r, rep) = st.geometry[&g.0];
                    let rep = match (rep, r) {
                        (Some(rep), _) => rep,
                        (None, R::Old(id)) => {
                            em.reference(geometry_rep(self.doc, id).ok_or_else(|| {
                                WriteError::Internal(format!(
                                    "geometry #{id} is in no representation"
                                ))
                            })?)?
                        }
                        (None, R::New(_)) => {
                            return Err(WriteError::Internal(
                                "new geometry without its representation".into(),
                            ));
                        }
                    };
                    (r, rep)
                }
            };
            out.push((rep, vec![item]));
        }
        Ok(out)
    }

    /// The usage of `item` in `rep` defining `definition`: a `geometric_item_specific_usage`.
    fn usage(em: &mut Emitter<'_>, definition: R, rep: R, item: R) -> Result<R, WriteError> {
        em.simple(
            "geometric_item_specific_usage",
            &[
                ("name", s("")),
                ("description", s("")),
                ("definition", definition.a()),
                ("used_representation", rep.a()),
                ("identified_item", item.a()),
            ],
        )
    }

    /// The usages of a feature's items: one `geometric_item_specific_usage` per item
    /// (maintainer decision 2026-10-09, which OpenCascade reads; the PMI practice's §5.1
    /// Figure 5 alternative, one `item_identified_representation_usage` of a
    /// `set_representation_item`, it drops). An aspect has one usage per representation
    /// (item_identified_representation_usage UR2), so a feature of several items in one
    /// representation identifies each through a member shape aspect of that one item, which
    /// it is composed of (`shape_aspect_relationship`, §5.1: one shape aspect and usage per
    /// face, shared by every feature on that face); the reader reads the composition as the
    /// feature's items. One item is identified once per representation (UR1): an item several
    /// features share, or that an existing shape aspect of the file already identifies, is
    /// such a member too, owned by a referenced feature of that item alone where there is one.
    /// A kept plain shape aspect of the file that identifies exactly all the feature's items
    /// in one representation by one usage (as earlier writes and §6.5.1 identify several faces)
    /// is the feature's one member instead.
    fn usages(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        feature: R,
        own: Option<usize>,
        items: &[Anchor],
    ) -> Result<(), WriteError> {
        let keys = self.usage_keys(em, st, items)?;
        let mut per_rep: BTreeMap<R, usize> = BTreeMap::new();
        for (rep, _) in &keys {
            *per_rep.entry(*rep).or_insert(0) += 1;
        }
        if let [(rep, n)] = per_rep.iter().map(|(r, n)| (*r, *n)).collect::<Vec<_>>()[..]
            && n > 1
            && keys
                .iter()
                .all(|k| !st.shared_keys.contains(k) && !st.owners.contains_key(k))
        {
            let all: Vec<R> = keys.iter().flat_map(|(_, i)| i.iter().copied()).collect();
            if let Some(m) = self.existing_owner(em, rep, &all)? {
                em.simple(
                    "shape_aspect_relationship",
                    &[
                        ("name", s("")),
                        ("description", s("")),
                        ("relating_shape_aspect", feature.a()),
                        ("related_shape_aspect", m.a()),
                    ],
                )?;
                return Ok(());
            }
        }
        for (rep, items) in keys {
            let [item] = items[..] else {
                return Err(WriteError::Internal("a usage key of several items".into()));
            };
            let several = per_rep[&rep] > 1;
            let key = (rep, items);
            let member = if let Some(&m) = st.shared.get(&key) {
                Some(m)
            } else if st.shared_keys.contains(&key)
                && own == st.owners.get(&key).copied()
                && own.is_some()
            {
                // This feature owns the shared item: the others are composed of it.
                Self::usage(em, feature, rep, item)?;
                st.shared.insert(key, feature);
                continue;
            } else if let Some(&o) = st.owners.get(&key) {
                Some(self.feature(em, st, pds, o)?)
            } else if let Some(m) = self.existing_owner(em, rep, &key.1)? {
                Some(m)
            } else if st.shared_keys.contains(&key) || several {
                let m = em.simple(
                    "shape_aspect",
                    &[
                        ("name", s("")),
                        ("description", s("")),
                        ("of_shape", pds.a()),
                        ("product_definitional", boolean(true)),
                    ],
                )?;
                Self::usage(em, m, rep, item)?;
                st.shared.insert(key, m);
                Some(m)
            } else {
                None
            };
            match member {
                Some(m) => {
                    em.simple(
                        "shape_aspect_relationship",
                        &[
                            ("name", s("")),
                            ("description", s("")),
                            ("relating_shape_aspect", feature.a()),
                            ("related_shape_aspect", m.a()),
                        ],
                    )?;
                }
                None => {
                    Self::usage(em, feature, rep, item)?;
                }
            }
        }
        Ok(())
    }

    /// A kept plain shape aspect of the part that identifies exactly `items` in `rep` (and
    /// nothing else) by one usage of the file.
    fn existing_owner(
        &self,
        em: &Emitter<'_>,
        rep: R,
        items: &[R],
    ) -> Result<Option<R>, WriteError> {
        let (R::Old(rep), Some(R::Old(first))) = (rep, items.first().copied()) else {
            return Ok(None);
        };
        let want: BTreeSet<u64> = items
            .iter()
            .filter_map(|r| match r {
                R::Old(n) => Some(*n),
                R::New(_) => None,
            })
            .collect();
        if want.len() != items.len() {
            return Ok(None);
        }
        let doc = self.doc;
        for &u in doc.referrers(first) {
            if em.removed.contains(&u)
                || !is_a(doc, u, "item_identified_representation_usage")
                || is_a(doc, u, "draughting_model_item_association")
                || attr_ref(
                    doc,
                    u,
                    "item_identified_representation_usage",
                    "used_representation",
                ) != Some(rep)
            {
                continue;
            }
            let got: BTreeSet<u64> = attr_refs(
                doc,
                u,
                "item_identified_representation_usage",
                "identified_item",
            )
            .into_iter()
            .collect();
            if got != want {
                continue;
            }
            let Some(owner) =
                attr_ref(doc, u, "item_identified_representation_usage", "definition")
            else {
                continue;
            };
            let plain = leaf_names(doc, owner) == ["shape_aspect"]
                && attr_ref(doc, owner, "shape_aspect", "of_shape") == Some(self.def.shape)
                && doc.referrers(owner).iter().all(|&r| {
                    r == u
                        || !is_a(doc, r, "item_identified_representation_usage")
                        || is_a(doc, r, "draughting_model_item_association")
                })
                && !doc.referrers(owner).iter().any(|&r| {
                    is_a(doc, r, "shape_aspect_relationship")
                        && attr_ref(doc, r, "shape_aspect_relationship", "relating_shape_aspect")
                            == Some(owner)
                });
            if plain && !em.removed.contains(&owner) {
                return Ok(Some(em.reference(owner)?));
            }
        }
        Ok(None)
    }

    fn dimension(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        i: usize,
    ) -> Result<R, WriteError> {
        let d = &self.pmi.dimensions[i];
        if let Some((&f, _)) = self
            .roles
            .iter()
            .find(|(_, r)| **r == Role::DatumFeatureOfSize(i))
        {
            self.feature(em, st, pds, f)?;
        }
        let dim = match st.dimensions.get(&i) {
            Some(&r) => r,
            None => {
                let sel = |a: &Option<AngleSelection>| {
                    a.map(|a| {
                        en(match a {
                            AngleSelection::Equal => "equal",
                            AngleSelection::Large => "large",
                            AngleSelection::Small => "small",
                        })
                    })
                };
                let r = match &d.kind {
                    DimensionKind::Size {
                        feature,
                        kind,
                        path,
                        angle,
                    } => {
                        let f = self.feature(em, st, pds, feature.0)?;
                        let p = match path {
                            Some(p) => Some(self.feature(em, st, pds, p.0)?),
                            None => None,
                        };
                        let mut types = vec!["dimensional_size"];
                        let mut vals = vec![
                            (("dimensional_size", "applies_to"), f.a()),
                            (("dimensional_size", "name"), s(kind.name())),
                        ];
                        if let Some(a) = sel(angle) {
                            types.push("angular_size");
                            vals.push((("angular_size", "angle_selection"), a));
                        }
                        if let Some(p) = p {
                            types.push("dimensional_size_with_path");
                            vals.push((("dimensional_size_with_path", "path"), p.a()));
                        }
                        em.instance(&types, &vals, &[])?
                    }
                    DimensionKind::Location {
                        from,
                        to,
                        kind,
                        path,
                        directed,
                        angle,
                    } => {
                        let a = self.feature(em, st, pds, from.0)?;
                        let b = self.feature(em, st, pds, to.0)?;
                        let p = match path {
                            Some(p) => Some(self.feature(em, st, pds, p.0)?),
                            None => None,
                        };
                        let mut types = vec!["dimensional_location"];
                        let mut vals = vec![
                            (("shape_aspect_relationship", "name"), s(kind.name())),
                            (("shape_aspect_relationship", "description"), s("")),
                            (
                                ("shape_aspect_relationship", "relating_shape_aspect"),
                                a.a(),
                            ),
                            (("shape_aspect_relationship", "related_shape_aspect"), b.a()),
                        ];
                        if let Some(x) = sel(angle) {
                            types.push("angular_location");
                            vals.push((("angular_location", "angle_selection"), x));
                        }
                        if let Some(p) = p {
                            types.push("dimensional_location_with_path");
                            vals.push((("dimensional_location_with_path", "path"), p.a()));
                        }
                        if *directed {
                            types.push("directed_dimensional_location");
                        }
                        em.instance(&types, &vals, &[])?
                    }
                };
                st.dimensions.insert(i, r);
                r
            }
        };
        if st.dimension_values.contains(&i) {
            return Ok(dim);
        }
        st.dimension_values.insert(i);

        // The value representation (§5.2).
        let mut items = Vec::new();
        if let Some(n) = &d.nominal {
            let mut q = Vec::new();
            if let Some(qual) = &d.qualifier {
                let name = match qual {
                    Qualifier::Maximum => "maximum",
                    Qualifier::Minimum => "minimum",
                    Qualifier::Average => "average",
                    Qualifier::Other(o) => o.as_str(),
                };
                q.push(em.simple("type_qualifier", &[("name", s(name))])?);
            }
            items.push(em.measure_item(
                "nominal value",
                measure_of_value(n),
                self.context,
                q,
                n.decimal_places,
            )?);
        }
        let limits = match &d.tolerance {
            DimTolerance::Limits(b) => Some(b),
            DimTolerance::Fit { limits, .. } => limits.as_ref(),
            _ => None,
        };
        if let Some(b) = limits {
            items.push(em.measure_item(
                "upper limit",
                measure_of_value(b.upper()),
                self.context,
                Vec::new(),
                b.upper().decimal_places,
            )?);
            items.push(em.measure_item(
                "lower limit",
                measure_of_value(b.lower()),
                self.context,
                Vec::new(),
                b.lower().decimal_places,
            )?);
        }
        if d.tolerance == DimTolerance::Basic {
            items.push(em.simple(
                "descriptive_representation_item",
                &[
                    ("name", s("dimensional note")),
                    ("description", s("theoretical")),
                ],
            )?);
        }
        if d.modifiers.contains(&DimensionModifier::Reference) {
            items.push(em.simple(
                "descriptive_representation_item",
                &[
                    ("name", s("dimensional note")),
                    ("description", s("auxiliary")),
                ],
            )?);
        }
        let mut type2 = Vec::new();
        for m in &d.modifiers {
            if let Some((text, _)) = DimensionModifier::TABLE.iter().find(|(_, x)| x == m) {
                type2.push(em.simple(
                    "descriptive_representation_item",
                    &[("name", s("dimensional note")), ("description", s(text))],
                )?);
            }
        }
        if !type2.is_empty() {
            items.push(em.simple(
                "compound_representation_item",
                &[
                    ("name", s("modifiers")),
                    (
                        "item_element",
                        typed("set_representation_item", refs(type2)),
                    ),
                ],
            )?);
        }
        let principle = match d.principle {
            None => "",
            Some(Principle::Independency) => "independency",
            Some(Principle::EnvelopeRequirement) => "envelope requirement",
        };
        let sdr = em.simple(
            "shape_dimension_representation",
            &[
                ("name", s(principle)),
                ("items", refs(items)),
                ("context_of_items", em.reference(self.context)?.a()),
            ],
        )?;
        em.simple(
            "dimensional_characteristic_representation",
            &[("dimension", dim.a()), ("representation", sdr.a())],
        )?;
        let range = match &d.tolerance {
            DimTolerance::Deviations(b) => Some(em.tolerance_value(self.context, b)?),
            DimTolerance::Fit { class, .. } => Some(em.limits_and_fits(class)?),
            _ => None,
        };
        if let Some(range) = range {
            em.simple(
                "plus_minus_tolerance",
                &[("range", range.a()), ("toleranced_dimension", dim.a())],
            )?;
        }
        Ok(dim)
    }

    fn tolerance(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        t: &GeometricTolerance,
        names: &mut BTreeSet<String>,
    ) -> Result<R, WriteError> {
        let target = match &t.target {
            ToleranceTarget::Feature(f) => self.feature(em, st, pds, f.0)?,
            ToleranceTarget::Dimension(d) => self.dimension(em, st, pds, d.0)?,
            ToleranceTarget::WholePart => pds,
            ToleranceTarget::Relation { .. } => {
                return Err(WriteError::Internal(
                    "a relation target passed the checks".into(),
                ));
            }
        };
        let system = match &t.datums {
            Some(ds) => Some(self.datum_system(em, st, pds, ds, names)?),
            None => None,
        };
        let mut types = vec![t.kind.entity()];
        let mut vals = vec![
            (("geometric_tolerance", "name"), s("")),
            (
                ("geometric_tolerance", "description"),
                s(t.description.as_deref().unwrap_or("")),
            ),
            (
                ("geometric_tolerance", "toleranced_shape_aspect"),
                target.a(),
            ),
        ];
        if let Some(m) = &t.magnitude {
            vals.push((
                ("geometric_tolerance", "magnitude"),
                em.value(self.context, m)?.a(),
            ));
        }
        if let Some(ds) = system {
            types.push("geometric_tolerance_with_datum_reference");
            vals.push((
                ("geometric_tolerance_with_datum_reference", "datum_system"),
                refs([ds]),
            ));
        }
        if !t.modifiers.is_empty() || t.maximum.is_some() {
            types.push("geometric_tolerance_with_modifiers");
            vals.push((
                ("geometric_tolerance_with_modifiers", "modifiers"),
                A::List(t.modifiers.iter().map(|m| en(m.as_enum())).collect()),
            ));
        }
        if let Some(m) = &t.maximum {
            types.push("geometric_tolerance_with_maximum_tolerance");
            vals.push((
                (
                    "geometric_tolerance_with_maximum_tolerance",
                    "maximum_upper_tolerance",
                ),
                em.value(self.context, m)?.a(),
            ));
        }
        if let Some(u) = &t.unit_basis {
            types.push("geometric_tolerance_with_defined_unit");
            vals.push((
                ("geometric_tolerance_with_defined_unit", "unit_size"),
                em.value(self.context, &u.size)?.a(),
            ));
            if let Some(area) = &u.area {
                types.push("geometric_tolerance_with_defined_area_unit");
                let shape = match area.shape {
                    AreaShape::Circular => "circular",
                    AreaShape::Square => "square",
                    AreaShape::Rectangular => "rectangular",
                    AreaShape::Cylindrical => "cylindrical",
                    AreaShape::Spherical => "spherical",
                };
                vals.push((
                    ("geometric_tolerance_with_defined_area_unit", "area_type"),
                    en(shape),
                ));
                if let Some(second) = &area.second {
                    vals.push((
                        (
                            "geometric_tolerance_with_defined_area_unit",
                            "second_unit_size",
                        ),
                        em.value(self.context, second)?.a(),
                    ));
                }
            }
        }
        if let Some(u) = &t.unequal {
            types.push("unequally_disposed_geometric_tolerance");
            vals.push((
                ("unequally_disposed_geometric_tolerance", "displacement"),
                em.value(self.context, u)?.a(),
            ));
        }
        let tol = em.instance(&types, &vals, &[])?;
        if let Some(z) = &t.zone {
            let form = em.simple(
                "tolerance_zone_form",
                &[("name", s(zone_form_name(&z.form)))],
            )?;
            let zone = em.simple(
                "tolerance_zone",
                &[
                    ("name", s("")),
                    ("description", s("")),
                    ("of_shape", pds.a()),
                    ("product_definitional", boolean(false)),
                    ("defining_tolerance", refs([tol])),
                    ("form", form.a()),
                ],
            )?;
            if let Some(pz) = &z.projected {
                let end = match pz.end {
                    Some(f) => self.feature(em, st, pds, f.0)?,
                    None => {
                        return Err(WriteError::Internal(
                            "a projected zone without its end passed the checks".into(),
                        ));
                    }
                };
                let len = em.value(self.context, &pz.length)?;
                em.simple(
                    "projected_zone_definition",
                    &[
                        ("zone", zone.a()),
                        ("boundaries", A::List(Vec::new())),
                        ("projection_end", end.a()),
                        ("projected_length", len.a()),
                    ],
                )?;
            }
            if let Some(bs) = &z.non_uniform {
                let mut fs = Vec::new();
                for f in bs {
                    fs.push(self.feature(em, st, pds, f.0)?);
                }
                em.simple(
                    "non_uniform_zone_definition",
                    &[("zone", zone.a()), ("boundaries", refs(fs))],
                )?;
            }
            if let Some(a) = &z.runout_angle {
                let angle = em.value(self.context, a)?;
                let o = em.simple("runout_zone_orientation", &[("angle", angle.a())])?;
                em.simple(
                    "runout_zone_definition",
                    &[
                        ("zone", zone.a()),
                        ("boundaries", A::List(Vec::new())),
                        ("orientation", o.a()),
                    ],
                )?;
            }
        }
        for a in &t.auxiliary {
            let v = match a {
                AuxiliaryClassification::AllOver => "all_over",
                AuxiliaryClassification::UnlessOtherwiseSpecified => "unless_otherwise_specified",
            };
            em.simple(
                "geometric_tolerance_auxiliary_classification",
                &[("attribute_value", en(v)), ("described_item", tol.a())],
            )?;
        }
        Ok(tol)
    }

    /// One `datum_system` per distinct system of the part (a value, §6.9.7), its compartments
    /// its own.
    fn datum_system(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        ds: &DatumSystem,
        names: &mut BTreeSet<String>,
    ) -> Result<R, WriteError> {
        let key = format!("{ds:?}");
        if let Some(&r) = st.systems.get(&key) {
            return Ok(r);
        }
        let label = |d: DatumId| self.pmi.datums[d.0].label().as_str().to_string();
        let base_name = ds
            .compartments()
            .iter()
            .map(|c| {
                c.references
                    .iter()
                    .map(|r| label(r.datum))
                    .collect::<Vec<_>>()
                    .join("-")
            })
            .collect::<Vec<_>>()
            .join("|");
        let mut name = base_name.clone();
        let mut n = 2;
        while names.contains(&name) {
            name = format!("{base_name} ({n})");
            n += 1;
        }
        names.insert(name.clone());
        let mut compartments = Vec::new();
        for c in ds.compartments() {
            let shape = || -> Vec<((&'static str, &'static str), A)> {
                vec![
                    (("shape_aspect", "name"), s("")),
                    (("shape_aspect", "description"), s("")),
                    (("shape_aspect", "of_shape"), pds.a()),
                    (("shape_aspect", "product_definitional"), boolean(false)),
                ]
            };
            let (base, modifiers) = match c.references.as_slice() {
                [one] => {
                    // One datum: its own modifiers and the compartment's are the same thing.
                    let mut m = c.modifiers.clone();
                    m.extend(one.modifiers.iter().cloned());
                    (st.datums[&one.datum.0].a(), m)
                }
                many => {
                    let mut elements = Vec::new();
                    for r in many {
                        let mut vals = shape();
                        vals.push((
                            ("general_datum_reference", "base"),
                            st.datums[&r.datum.0].a(),
                        ));
                        if !r.modifiers.is_empty() {
                            let m = self.datum_modifiers(em, &r.modifiers)?;
                            vals.push((("general_datum_reference", "modifiers"), m));
                        }
                        elements.push(em.instance(&["datum_reference_element"], &vals, &[])?);
                    }
                    (
                        typed("common_datum_list", refs(elements)),
                        c.modifiers.clone(),
                    )
                }
            };
            let mut vals = shape();
            vals.push((("general_datum_reference", "base"), base));
            if !modifiers.is_empty() {
                let m = self.datum_modifiers(em, &modifiers)?;
                vals.push((("general_datum_reference", "modifiers"), m));
            }
            compartments.push(em.instance(&["datum_reference_compartment"], &vals, &[])?);
        }
        let r = em.simple(
            "datum_system",
            &[
                ("name", s(&name)),
                ("description", s("")),
                ("of_shape", pds.a()),
                ("product_definitional", boolean(false)),
                ("constituents", refs(compartments)),
            ],
        )?;
        st.systems.insert(key, r);
        Ok(r)
    }

    fn datum_modifiers(&self, em: &mut Emitter<'_>, ms: &[DatumModifier]) -> Result<A, WriteError> {
        let mut out = Vec::new();
        for m in ms {
            out.push(match m {
                DatumModifier::Simple(x) => {
                    let name = SimpleDatumModifier::ALL
                        .iter()
                        .find(|(_, v)| v == x)
                        .map_or("", |(n, _)| n);
                    typed("simple_datum_reference_modifier", en(name))
                }
                DatumModifier::WithValue { kind, value } => {
                    let k = match kind {
                        DatumModifierType::CircularOrCylindrical => "circular_or_cylindrical",
                        DatumModifierType::Spherical => "spherical",
                        DatumModifierType::Distance => "distance",
                        DatumModifierType::Projected => "projected",
                    };
                    let v = em.value(self.context, value)?;
                    em.simple(
                        "datum_reference_modifier_with_value",
                        &[("modifier_type", en(k)), ("modifier_value", v.a())],
                    )?
                    .a()
                }
            });
        }
        Ok(A::List(out))
    }

    /// The parameters of a thread or knurl: a `shape_representation_with_parameters` named
    /// items, as their WHERE rules name them.
    fn parameters(
        &self,
        em: &mut Emitter<'_>,
        definition: R,
        items: Vec<R>,
    ) -> Result<(), WriteError> {
        let pdef = em.simple(
            "property_definition",
            &[
                ("name", s("")),
                ("description", s("")),
                ("definition", definition.a()),
            ],
        )?;
        let rep = em.simple(
            "shape_representation_with_parameters",
            &[
                ("name", s("")),
                ("items", refs(items)),
                ("context_of_items", em.reference(self.context)?.a()),
            ],
        )?;
        em.simple(
            "property_definition_representation",
            &[("definition", pdef.a()), ("used_representation", rep.a())],
        )?;
        Ok(())
    }

    fn text_item(em: &mut Emitter<'_>, name: &str, text: &str) -> Result<R, WriteError> {
        em.simple(
            "descriptive_representation_item",
            &[("name", s(name)), ("description", s(text))],
        )
    }

    /// The feature definition's own shape and its 'applied shape' occurrence (thread WR13).
    fn applied(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        definition: R,
        feature: FeatureId,
    ) -> Result<R, WriteError> {
        let shape = em.simple(
            "product_definition_shape",
            &[
                ("name", s("")),
                ("description", s("")),
                ("definition", definition.a()),
            ],
        )?;
        let f = self.feature(em, st, pds, feature.0)?;
        let occ = em.simple(
            "shape_aspect",
            &[
                ("name", s("")),
                ("description", s("applied shape")),
                ("of_shape", shape.a()),
                ("product_definitional", boolean(false)),
            ],
        )?;
        em.simple(
            "shape_defining_relationship",
            &[
                ("name", s("")),
                ("description", s("applied shape")),
                ("relating_shape_aspect", f.a()),
                ("related_shape_aspect", occ.a()),
            ],
        )?;
        Ok(shape)
    }

    /// A shape aspect of the definition's shape described `description`, and, when `from` is
    /// given, the `shape_defining_relationship` described `usage` from it.
    fn definition_aspect(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        shape: R,
        (description, usage): (&str, &str),
        from: Option<R>,
    ) -> Result<(), WriteError> {
        let occ = em.simple(
            "shape_aspect",
            &[
                ("name", s("")),
                ("description", s(description)),
                ("of_shape", shape.a()),
                ("product_definitional", boolean(false)),
            ],
        )?;
        let _ = (st, pds);
        if let Some(f) = from {
            em.simple(
                "shape_defining_relationship",
                &[
                    ("name", s("")),
                    ("description", s(usage)),
                    ("relating_shape_aspect", f.a()),
                    ("related_shape_aspect", occ.a()),
                ],
            )?;
        }
        Ok(())
    }

    fn thread(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        t: &Thread,
    ) -> Result<(), WriteError> {
        let def = em.simple("thread", &[("name", s("")), ("description", s(""))])?;
        let shape = self.applied(em, st, pds, def, t.feature)?;
        let area = match t.partial_area {
            Some(f) => Some(self.role_instance(em, st, pds, f, Role::AppliedArea)?),
            None => None,
        };
        self.definition_aspect(
            em,
            st,
            pds,
            shape,
            ("partial area occurrence", "applied area usage"),
            area,
        )?;
        let runout = match t.runout {
            Some(f) => Some(self.role_instance(em, st, pds, f, Role::ThreadRunout)?),
            None => None,
        };
        self.definition_aspect(
            em,
            st,
            pds,
            shape,
            ("thread runout", "thread runout usage"),
            runout,
        )?;
        let ctx = self.context;
        let hand = |h: Hand| match h {
            Hand::Left => "left",
            Hand::Right => "right",
        };
        let mut items = vec![
            Self::text_item(
                em,
                "thread side",
                match t.side {
                    ThreadSide::Internal => "internal",
                    ThreadSide::External => "external",
                },
            )?,
            em.measure_item(
                "major diameter",
                MeasureItem::Length(&t.major_diameter),
                ctx,
                Vec::new(),
                None,
            )?,
        ];
        if let Some(l) = &t.minor_diameter {
            items.push(em.measure_item(
                "minor diameter",
                MeasureItem::Length(l),
                ctx,
                Vec::new(),
                None,
            )?);
        }
        if let Some(l) = &t.pitch_diameter {
            items.push(em.measure_item(
                "pitch diameter",
                MeasureItem::Length(l),
                ctx,
                Vec::new(),
                None,
            )?);
        }
        items.push(em.measure_item(
            "number of threads",
            MeasureItem::Ratio(&t.number_of_threads.0),
            ctx,
            Vec::new(),
            None,
        )?);
        items.push(Self::text_item(em, "form", &t.form)?);
        items.push(Self::text_item(em, "fit class", &t.fit_class)?);
        if let Some(c) = &t.fit_class_2 {
            items.push(Self::text_item(em, "fit class 2", c)?);
        }
        items.push(Self::text_item(em, "hand", hand(t.hand))?);
        if let Some(l) = &t.crest {
            items.push(em.measure_item("crest", MeasureItem::Length(l), ctx, Vec::new(), None)?);
        }
        if let Some(q) = &t.qualifier {
            items.push(Self::text_item(em, "qualifier", q)?);
        }
        if let Some(l) = &t.nominal_size {
            items.push(em.measure_item(
                "nominal size",
                MeasureItem::Length(l),
                ctx,
                Vec::new(),
                None,
            )?);
        }
        self.parameters(em, def, items)
    }

    fn knurl(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        k: &Knurl,
    ) -> Result<(), WriteError> {
        let pattern = match k.pattern {
            KnurlPattern::Diamond => "diamond",
            KnurlPattern::Diagonal => "diagonal",
            KnurlPattern::Straight => "straight",
        };
        let def = em.simple(
            "turned_knurl",
            &[("name", s("")), ("description", s(pattern))],
        )?;
        let shape = self.applied(em, st, pds, def, k.feature)?;
        let area = self.role_instance(em, st, pds, k.feature, Role::AppliedArea)?;
        // turned_knurl WR11 (as thread WR12): exactly one 'partial area occurrence' with one
        // 'applied area usage' from an applied_area: the knurled faces themselves (the model
        // holds no other area for a knurl).
        self.definition_aspect(
            em,
            st,
            pds,
            shape,
            ("partial area occurrence", "applied area usage"),
            Some(area),
        )?;
        let ctx = self.context;
        let mut items = vec![
            em.measure_item(
                "major diameter",
                MeasureItem::Length(&k.major_diameter),
                ctx,
                Vec::new(),
                None,
            )?,
            em.measure_item(
                "nominal diameter",
                MeasureItem::Length(&k.nominal_diameter),
                ctx,
                Vec::new(),
                None,
            )?,
            em.measure_item(
                "diametral pitch",
                MeasureItem::Length(&k.diametral_pitch),
                ctx,
                Vec::new(),
                None,
            )?,
        ];
        let teeth;
        if let Some(c) = k.number_of_teeth {
            teeth = Decimal::parse(&format!("{}.", c.0)).map_err(|e| WriteError::Internal(e.0))?;
            items.push(em.measure_item(
                "number of teeth",
                MeasureItem::Count(&teeth),
                ctx,
                Vec::new(),
                None,
            )?);
        }
        if let Some(l) = &k.tooth_depth {
            items.push(em.measure_item(
                "tooth depth",
                MeasureItem::Length(l),
                ctx,
                Vec::new(),
                None,
            )?);
        }
        if let Some(l) = &k.root_fillet {
            items.push(em.measure_item(
                "root fillet",
                MeasureItem::Length(l),
                ctx,
                Vec::new(),
                None,
            )?);
        }
        if let Some(a) = &k.helix_angle {
            items.push(em.measure_item(
                "helix angle",
                MeasureItem::Angle(a),
                ctx,
                Vec::new(),
                None,
            )?);
        }
        if let Some(h) = k.helix_hand {
            items.push(Self::text_item(
                em,
                "helix hand",
                match h {
                    Hand::Left => "left",
                    Hand::Right => "right",
                },
            )?);
        }
        self.parameters(em, def, items)
    }

    /// A user defined attribute (UDA practice §5–§7): `general_property` associated with the
    /// property definition, its values the items of its representation.
    fn attribute_set(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pd: R,
        pds: R,
        a: &AttributeSet,
    ) -> Result<(), WriteError> {
        let owner = match a.on {
            // Editable note text on the part is on its shape (PMI practice §7.4.3, Table 17:
            // 'on part' is property_definition.definition = product_definition_shape).
            None if a.name == "semantic text" => pds,
            None => pd,
            Some(NoteOwner::Feature(f)) => self.feature(em, st, pds, f.0)?,
            Some(NoteOwner::Dimension(d)) => self.dimension(em, st, pds, d.0)?,
            Some(NoteOwner::Tolerance(t)) => st.tolerances[&t.0],
            Some(NoteOwner::DatumTarget(_)) => {
                return Err(WriteError::Internal(
                    "an attribute on a datum target passed the checks".into(),
                ));
            }
        };
        let mut items = Vec::new();
        for (key, v) in &a.items {
            items.push(match v {
                AttributeValue::Text(t) => Self::text_item(em, key, t)?,
                AttributeValue::Integer(i) => em.simple(
                    "integer_representation_item",
                    &[("name", s(key)), ("the_value", A::Integer(*i))],
                )?,
                AttributeValue::Real(d) => {
                    let v = real(d).map_err(WriteError::Internal)?;
                    em.instance(
                        &["real_representation_item"],
                        &[(("", "name"), s(key)), (("", "the_value"), v)],
                        &[d],
                    )?
                }
                AttributeValue::Boolean(b) => em.simple(
                    "boolean_representation_item",
                    &[("name", s(key)), ("the_value", boolean(*b))],
                )?,
                AttributeValue::Measure(m) => {
                    em.measure_item(key, measure_of_value(m), self.context, Vec::new(), None)?
                }
                AttributeValue::OtherMeasure {
                    measure,
                    value,
                    unit,
                } => {
                    let v = typed(measure, real(value).map_err(WriteError::Internal)?);
                    if unit.is_empty() {
                        em.instance(
                            &["value_representation_item"],
                            &[(("", "name"), s(key)), (("", "value_component"), v)],
                            &[value],
                        )?
                    } else {
                        let u = self.other_unit(unit).map_err(WriteError::Internal)?;
                        let u = em.reference(u)?;
                        em.instance(
                            &["measure_representation_item"],
                            &[
                                (("", "name"), s(key)),
                                (("", "value_component"), v),
                                (("", "unit_component"), u.a()),
                            ],
                            &[value],
                        )?
                    }
                }
            });
        }
        let gp = em.simple(
            "general_property",
            &[
                ("id", s("")),
                ("name", s(&a.name)),
                ("description", A::Unset),
            ],
        )?;
        let pdef = self.property(em, owner, &a.name, "user defined attribute", "", &items)?;
        em.simple(
            "general_property_association",
            &[
                ("name", s("")),
                ("description", A::Unset),
                ("base_definition", gp.a()),
                ("derived_definition", pdef.a()),
            ],
        )?;
        Ok(())
    }

    /// A surface texture as ISO 10303-1110 maps it (the PMI practice has no section on it):
    /// the `Surface_texture` is `property_definition('surface texture', '', <faces' shape
    /// aspect or the part's shape>)` with a `representation('surface texture')` (the global
    /// rule restrict_representation_for_surface_condition: the names agree) holding the
    /// 'material removal condition'; each `Standard_surface_texture_parameter` is a
    /// `property_definition` on the same owner, related to it by
    /// `property_definition_relationship('surface texture parameter')`, with a
    /// `surface_texture_representation` of the characteristic ('measuring method', WR2) and its
    /// value (a length measure item, WR1, WR3), and associated with a `general_property`
    /// 'surface_condition' (WR5). The parameter is named 'surface_condition', not the mapping's
    /// 'surface texture parameter': general_property_association WR2 requires the derived and
    /// base names to agree, so with WR5 no other name is schema-valid (docs/step-ap242.md,
    /// question 3).
    fn surface_texture(
        &self,
        em: &mut Emitter<'_>,
        st: &mut Emitted,
        pds: R,
        t: &SurfaceTexture,
    ) -> Result<(), WriteError> {
        let owner = match t.on {
            None => pds,
            Some(f) => self.feature(em, st, pds, f.0)?,
        };
        let removal = Self::text_item(em, "material removal condition", t.material_removal.term())?;
        let texture = self.property(
            em,
            owner,
            "surface texture",
            "",
            "surface texture",
            &[removal],
        )?;
        for p in &t.parameters {
            let method = Self::text_item(em, "measuring method", &p.characteristic)?;
            let value = em.measure_item(
                "characteristic value",
                MeasureItem::Length(&p.value),
                self.context,
                Vec::new(),
                None,
            )?;
            let parameter = em.simple(
                "property_definition",
                &[
                    ("name", s("surface_condition")),
                    ("description", s("")),
                    ("definition", owner.a()),
                ],
            )?;
            let rep = em.simple(
                "surface_texture_representation",
                &[
                    ("name", s("surface texture parameter")),
                    ("items", refs([method, value])),
                    ("context_of_items", em.reference(self.context)?.a()),
                ],
            )?;
            em.simple(
                "property_definition_representation",
                &[
                    ("definition", parameter.a()),
                    ("used_representation", rep.a()),
                ],
            )?;
            em.simple(
                "property_definition_relationship",
                &[
                    ("name", s("surface texture parameter")),
                    ("description", s("")),
                    ("relating_property_definition", texture.a()),
                    ("related_property_definition", parameter.a()),
                ],
            )?;
            let condition = em.simple(
                "general_property",
                &[
                    ("id", s("")),
                    ("name", s("surface_condition")),
                    ("description", A::Unset),
                ],
            )?;
            em.simple(
                "general_property_association",
                &[
                    ("name", s("")),
                    ("description", A::Unset),
                    ("base_definition", condition.a()),
                    ("derived_definition", parameter.a()),
                ],
            )?;
        }
        Ok(())
    }
}

/// What a part's emission has made so far.
#[derive(Default)]
struct Emitted {
    /// Canonical feature → instance.
    features: BTreeMap<usize, R>,
    /// Geometry → its item and, for a new one, the representation holding it.
    geometry: BTreeMap<usize, (R, Option<R>)>,
    datums: BTreeMap<usize, R>,
    dimensions: BTreeMap<usize, R>,
    dimension_values: BTreeSet<usize>,
    tolerances: BTreeMap<usize, R>,
    systems: BTreeMap<String, R>,
    /// Usage keys (representation, items) several features need: each one shape aspect.
    shared_keys: BTreeSet<(R, Vec<R>)>,
    shared: BTreeMap<(R, Vec<R>), R>,
    /// Per shared key, the feature that owns it (a referenced feature of exactly those items).
    owners: BTreeMap<(R, Vec<R>), usize>,
    twins: BTreeMap<(usize, Role), R>,
}

fn zone_form_name(f: &ZoneForm) -> &str {
    match f {
        ZoneForm::Other(o) => o,
        f => ZoneForm::TABLE
            .iter()
            .find(|(_, x)| x == f)
            .map_or("", |(n, _)| n),
    }
}

/// Every REAL of a geometry item is a Part 21 REAL.
fn geometry_values(g: &GeometryItem) -> Result<(), String> {
    fn value(v: &GeometryValue) -> Result<(), String> {
        match v {
            GeometryValue::Real(d) => real(d).map(|_| ()),
            GeometryValue::Item(i) => geometry_values(i),
            GeometryValue::List(l) => l.iter().try_for_each(value),
            GeometryValue::Typed { value: v, .. } => value(v),
            _ => Ok(()),
        }
    }
    g.leaves
        .iter()
        .try_for_each(|(_, vals)| vals.iter().try_for_each(value))
}

/// A supplemental geometry item, by value: its sub-items first (equal ones once).
fn geometry_item(
    em: &mut Emitter<'_>,
    g: &GeometryItem,
    memo: &mut HashMap<GeometryItem, R>,
) -> Result<R, WriteError> {
    if let Some(&r) = memo.get(g) {
        return Ok(r);
    }
    fn value(
        em: &mut Emitter<'_>,
        v: &GeometryValue,
        memo: &mut HashMap<GeometryItem, R>,
        stated: &mut Vec<Decimal>,
    ) -> Result<A, WriteError> {
        Ok(match v {
            GeometryValue::Real(d) => {
                stated.push(d.clone());
                real(d).map_err(WriteError::Internal)?
            }
            GeometryValue::Integer(i) => A::Integer(*i),
            GeometryValue::Text(t) => s(t),
            GeometryValue::Enum(e) => A::Enum(e.clone()),
            GeometryValue::Binary(b) => A::Binary(b.clone()),
            GeometryValue::Item(i) => geometry_item(em, i, memo)?.a(),
            GeometryValue::List(l) => {
                let mut out = Vec::new();
                for x in l {
                    out.push(value(em, x, memo, stated)?);
                }
                A::List(out)
            }
            GeometryValue::Typed {
                type_name,
                value: v,
            } => A::Typed {
                type_name: type_name.clone(),
                value: Box::new(value(em, v, memo, stated)?),
            },
            GeometryValue::Unset => A::Unset,
            GeometryValue::Derived => A::Derived,
        })
    }
    let mut stated = Vec::new();
    let mut leaves = Vec::new();
    for (name, vals) in &g.leaves {
        let mut attrs = Vec::new();
        for v in vals {
            attrs.push(value(em, v, memo, &mut stated)?);
        }
        leaves.push((name.clone(), attrs));
    }
    let record = match leaves.as_slice() {
        [(name, attrs)] => p21::simple(name, attrs.clone()),
        _ => Complex::new(leaves.into_iter().map(|(n, a)| p21::leaf(&n, a)))?.into_record(),
    };
    let refs: Vec<&Decimal> = stated.iter().collect();
    let r = em.push(record, &refs)?;
    memo.insert(g.clone(), r);
    Ok(r)
}

// ---------------------------------------------------------------------------------------------
// Datum feature symbols (decision 6): presentation derived from the semantics
// ---------------------------------------------------------------------------------------------

/// The Hershey "Futura Light" single-stroke font (`futural.jhf`, James Hurt's format), as
/// specify-core carries it (from <https://github.com/kamalmostafa/hershey-fonts>). Its licence
/// asks that these acknowledgements go with the font data: the Hershey Fonts were originally
/// created by Dr. A. V. Hershey while working at the U. S. National Bureau of Standards; the
/// format of the font data was originally created by James Hurt, Cognition, Inc., 900
/// Technology Park Drive, Billerica, MA 01821. AP242 presentation carries no characters, only
/// geometry, so a datum letter is drawn as polylines.
const HERSHEY_FUTURAL: &str = r#"12345  1JZ
12345  9MWRFRT RRYQZR[SZRY
12345  6JZNFNM RVFVM
12345 12H]SBLb RYBRb RLOZO RKUYU
12345 27H\PBP_ RTBT_ RYIWGTFPFMGKIKKLMMNOOUQWRXSYUYXWZT[P[MZKX
12345 32F^[FI[ RNFPHPJOLMMKMIKIIJGLFNFPGSHVHYG[F RWTUUTWTYV[X[ZZ[X[VYTWT
12345 35E_\O\N[MZMYNXPVUTXRZP[L[JZIYHWHUISJRQNRMSKSIRGPFNGMIMKNNPQUXWZY[[[\Z\Y
12345  8MWRHQGRFSGSIRKQL
12345 11KYVBTDRGPKOPOTPYR]T`Vb
12345 11KYNBPDRGTKUPUTTYR]P`Nb
12345  9JZRLRX RMOWU RWOMU
12345  6E_RIR[ RIR[R
12345  8NVSWRXQWRVSWSYQ[
12345  3E_IR[R
12345  6NVRVQWRXSWRV
12345  3G][BIb
12345 18H\QFNGLJKOKRLWNZQ[S[VZXWYRYOXJVGSFQF
12345  5H\NJPISFS[
12345 15H\LKLJMHNGPFTFVGWHXJXLWNUQK[Y[
12345 16H\MFXFRNUNWOXPYSYUXXVZS[P[MZLYKW
12345  7H\UFKTZT RUFU[
12345 18H\WFMFLOMNPMSMVNXPYSYUXXVZS[P[MZLYKW
12345 24H\XIWGTFRFOGMJLOLTMXOZR[S[VZXXYUYTXQVOSNRNOOMQLT
12345  6H\YFO[ RKFYF
12345 30H\PFMGLILKMMONSOVPXRYTYWXYWZT[P[MZLYKWKTLRNPQOUNWMXKXIWGTFPF
12345 24H\XMWPURRSQSNRLPKMKLLINGQFRFUGWIXMXRWWUZR[P[MZLX
12345 12NVROQPRQSPRO RRVQWRXSWRV
12345 14NVROQPRQSPRO RSWRXQWRVSWSYQ[
12345  4F^ZIJRZ[
12345  6E_IO[O RIU[U
12345  4F^JIZRJ[
12345 21I[LKLJMHNGPFTFVGWHXJXLWNVORQRT RRYQZR[SZRY
12345 56E`WNVLTKQKOLNMMPMSNUPVSVUUVS RQKOMNPNSOUPV RWKVSVUXVZV\T]Q]O\L[JYHWGTFQFNGLHJJILHOHRIUJWLYNZQ[T[WZYYZX RXKWSWUXV
12345  9I[RFJ[ RRFZ[ RMTWT
12345 24G\KFK[ RKFTFWGXHYJYLXNWOTP RKPTPWQXRYTYWXYWZT[K[
12345 19H]ZKYIWGUFQFOGMILKKNKSLVMXOZQ[U[WZYXZV
12345 16G\KFK[ RKFRFUGWIXKYNYSXVWXUZR[K[
12345 12H[LFL[ RLFYF RLPTP RL[Y[
12345  9HZLFL[ RLFYF RLPTP
12345 23H]ZKYIWGUFQFOGMILKKNKSLVMXOZQ[U[WZYXZVZS RUSZS
12345  9G]KFK[ RYFY[ RKPYP
12345  3NVRFR[
12345 11JZVFVVUYTZR[P[NZMYLVLT
12345  9G\KFK[ RYFKT RPOY[
12345  6HYLFL[ RL[X[
12345 12F^JFJ[ RJFR[ RZFR[ RZFZ[
12345  9G]KFK[ RKFY[ RYFY[
12345 22G]PFNGLIKKJNJSKVLXNZP[T[VZXXYVZSZNYKXIVGTFPF
12345 14G\KFK[ RKFTFWGXHYJYMXOWPTQKQ
12345 25G]PFNGLIKKJNJSKVLXNZP[T[VZXXYVZSZNYKXIVGTFPF RSWY]
12345 17G\KFK[ RKFTFWGXHYJYLXNWOTPKP RRPY[
12345 21H\YIWGTFPFMGKIKKLMMNOOUQWRXSYUYXWZT[P[MZKX
12345  6JZRFR[ RKFYF
12345 11G]KFKULXNZQ[S[VZXXYUYF
12345  6I[JFR[ RZFR[
12345 12F^HFM[ RRFM[ RRFW[ R\FW[
12345  6H\KFY[ RYFK[
12345  7I[JFRPR[ RZFRP
12345  9H\YFK[ RKFYF RK[Y[
12345 12KYOBOb RPBPb ROBVB RObVb
12345  3KYKFY^
12345 12KYTBTb RUBUb RNBUB RNbUb
12345  6JZRDJR RRDZR
12345  3I[Ib[b
12345  8NVSKQMQORPSORNQO
12345 18I\XMX[ RXPVNTMQMONMPLSLUMXOZQ[T[VZXX
12345 18H[LFL[ RLPNNPMSMUNWPXSXUWXUZS[P[NZLX
12345 15I[XPVNTMQMONMPLSLUMXOZQ[T[VZXX
12345 18I\XFX[ RXPVNTMQMONMPLSLUMXOZQ[T[VZXX
12345 18I[LSXSXQWOVNTMQMONMPLSLUMXOZQ[T[VZXX
12345  9MYWFUFSGRJR[ ROMVM
12345 23I\XMX]W`VaTbQbOa RXPVNTMQMONMPLSLUMXOZQ[T[VZXX
12345 11I\MFM[ RMQPNRMUMWNXQX[
12345  9NVQFRGSFREQF RRMR[
12345 12MWRFSGTFSERF RSMS^RaPbNb
12345  9IZMFM[ RWMMW RQSX[
12345  3NVRFR[
12345 19CaGMG[ RGQJNLMOMQNRQR[ RRQUNWMZM\N]Q][
12345 11I\MMM[ RMQPNRMUMWNXQX[
12345 18I\QMONMPLSLUMXOZQ[T[VZXXYUYSXPVNTMQM
12345 18H[LMLb RLPNNPMSMUNWPXSXUWXUZS[P[NZLX
12345 18I\XMXb RXPVNTMQMONMPLSLUMXOZQ[T[VZXX
12345  9KXOMO[ ROSPPRNTMWM
12345 18J[XPWNTMQMNNMPNRPSUTWUXWXXWZT[Q[NZMX
12345  9MYRFRWSZU[W[ ROMVM
12345 11I\MMMWNZP[S[UZXW RXMX[
12345  6JZLMR[ RXMR[
12345 12G]JMN[ RRMN[ RRMV[ RZMV[
12345  6J[MMX[ RXMM[
12345 10JZLMR[ RXMR[P_NaLbKb
12345  9J[XMM[ RMMXM RM[X[
12345 40KYTBRCQDPFPHQJRKSMSOQQ RRCQEQGRISJTLTNSPORSTTVTXSZR[Q]Q_Ra RQSSUSWRYQZP\P^Q`RaTb
12345  3NVRBRb
12345 40KYPBRCSDTFTHSJRKQMQOSQ RRCSESGRIQJPLPNQPURQTPVPXQZR[S]S_Ra RSSQUQWRYSZT\T^S`RaPb
12345 24F^IUISJPLONOPPTSVTXTZS[Q RISJQLPNPPQTTVUXUZT[Q[O
12345 35JZJFJ[K[KFLFL[M[MFNFN[O[OFPFP[Q[QFRFR[S[SFTFT[U[UFVFV[W[WFXFX[Y[YFZFZ[
"#;

/// One glyph: its advance and its strokes, in font units (capital height 21, baseline 0).
type Glyph = (f64, Vec<Vec<(f64, f64)>>);

/// Character → glyph for ASCII 32 to 126: a 5-character number, a 3-character vertex count,
/// then pairs of characters each offset from 'R' (the left and right bearings, then x, y, with
/// " R" lifting the pen).
fn hershey_glyphs() -> Vec<Glyph> {
    let raw: Vec<u8> = HERSHEY_FUTURAL.bytes().filter(|&b| b != b'\n').collect();
    let mut glyphs = Vec::new();
    let mut i = 0;
    let at = |b: u8| f64::from(b) - 82.0;
    while i + 8 <= raw.len() {
        let count: usize = std::str::from_utf8(&raw[i + 5..i + 8])
            .ok()
            .and_then(|t| t.trim().parse().ok())
            .unwrap_or(0);
        let body = &raw[i + 8..(i + 8 + 2 * count).min(raw.len())];
        i += 8 + 2 * count;
        if body.len() < 2 {
            continue;
        }
        let (left, right) = (at(body[0]), at(body[1]));
        let mut strokes = Vec::new();
        let mut current = Vec::new();
        for pair in body[2..].chunks(2) {
            if pair == b" R" {
                strokes.push(std::mem::take(&mut current));
            } else if let [x, y] = pair {
                current.push((at(*x) - left, 9.0 - at(*y)));
            }
        }
        strokes.push(current);
        strokes.retain(|s: &Vec<(f64, f64)>| s.len() > 1);
        glyphs.push((right - left, strokes));
    }
    glyphs
}

/// The strokes of `text` at capital `height`, from x = 0 on the baseline, and its width; a
/// character the font lacks is drawn as '?'.
fn lettering(text: &str, height: f64) -> (Vec<Vec<(f64, f64)>>, f64) {
    let glyphs = hershey_glyphs();
    let scale = height / 21.0;
    let mut out = Vec::new();
    let mut x = 0.0;
    for c in text.chars() {
        let k = (c as usize)
            .checked_sub(32)
            .filter(|&k| k < 95 && k < glyphs.len())
            .unwrap_or(31);
        let (advance, strokes) = &glyphs[k];
        for s in strokes {
            out.push(
                s.iter()
                    .map(|(px, py)| ((x + px) * scale, py * scale))
                    .collect(),
            );
        }
        x += advance;
    }
    (out, x * scale)
}

/// The coordinates of a `cartesian_point` instance.
fn point_of(doc: &Document, id: u64) -> Option<[f64; 3]> {
    if !leaf_names(doc, id).iter().any(|n| n == "cartesian_point") {
        return None;
    }
    let Some(Attribute::List(c)) = attr(doc, id, "cartesian_point", "coordinates") else {
        return None;
    };
    let mut p = [0.0; 3];
    for (k, v) in c.iter().take(3).enumerate() {
        p[k] = match v {
            Attribute::Real(x) => *x,
            Attribute::Integer(i) => *i as f64,
            _ => return None,
        };
    }
    Some(p)
}

/// The first point under `id`, depth first in attribute order: for a face, a vertex of its
/// first bound (bounds come before the surface), for an edge its start vertex, for a geometric
/// item its location.
fn first_point(doc: &Document, id: u64) -> Option<[f64; 3]> {
    let mut stack = vec![id];
    let mut seen = BTreeSet::new();
    while let Some(x) = stack.pop() {
        if !seen.insert(x) {
            continue;
        }
        if let Some(p) = point_of(doc, x) {
            return Some(p);
        }
        let mut next = Vec::new();
        if let Some(e) = doc.get(x) {
            p21::visit_attributes(e, &mut |a| {
                if let Attribute::EntityRef(n) = a {
                    next.push(*n);
                }
            });
        }
        stack.extend(next.into_iter().rev());
    }
    None
}

/// The vertices of `faces`: the points of their `vertex_point`s, reached through their bounds
/// (curves and surfaces are not entered).
fn vertices(doc: &Document, faces: &[u64]) -> Vec<[f64; 3]> {
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    let mut stack: Vec<u64> = faces
        .iter()
        .flat_map(|&f| attr_refs(doc, f, "face", "bounds"))
        .collect();
    while let Some(x) = stack.pop() {
        if !seen.insert(x) {
            continue;
        }
        if leaf_names(doc, x).iter().any(|n| n == "vertex_point") {
            if let Some(p) =
                attr_ref(doc, x, "vertex_point", "vertex_geometry").and_then(|p| point_of(doc, p))
            {
                out.push(p);
            }
            continue;
        }
        if is_a(doc, x, "curve") || is_a(doc, x, "surface") || is_a(doc, x, "point") {
            continue;
        }
        if let Some(e) = doc.get(x) {
            p21::visit_attributes(e, &mut |a| {
                if let Attribute::EntityRef(n) = a {
                    stack.push(*n);
                }
            });
        }
    }
    out
}

/// The first `cartesian_point` of a supplemental geometry item, by value.
fn geometry_point(g: &GeometryItem) -> Option<[f64; 3]> {
    fn value(v: &GeometryValue) -> Option<[f64; 3]> {
        match v {
            GeometryValue::Item(i) => geometry_point(i),
            GeometryValue::List(l) => l.iter().find_map(value),
            GeometryValue::Typed { value: v, .. } => value(v),
            _ => None,
        }
    }
    for (name, vals) in &g.leaves {
        if name.eq_ignore_ascii_case("cartesian_point")
            && let Some(GeometryValue::List(c)) = vals.get(1)
        {
            let mut p = [0.0; 3];
            for (k, v) in c.iter().take(3).enumerate() {
                p[k] = match v {
                    GeometryValue::Real(d) => d.to_f64(),
                    GeometryValue::Integer(i) => *i as f64,
                    _ => return None,
                };
            }
            return Some(p);
        }
    }
    g.leaves
        .iter()
        .find_map(|(_, vals)| vals.iter().find_map(value))
}

/// Where datum feature symbols go (as specify-core lays them out): standing above the part in
/// the plane of its two longest extents, each in the plane through its feature's point, side
/// by side, sized to the part.
struct SymbolLayout {
    hi: [f64; 3],
    /// Axes of the longest, second longest and shortest extent.
    u: usize,
    v: usize,
    height: f64,
    /// The right edge of the last symbol.
    right: f64,
}

impl SymbolLayout {
    fn new(points: &[[f64; 3]]) -> SymbolLayout {
        let mut lo = [f64::INFINITY; 3];
        let mut hi = [f64::NEG_INFINITY; 3];
        for p in points {
            for k in 0..3 {
                lo[k] = lo[k].min(p[k]);
                hi[k] = hi[k].max(p[k]);
            }
        }
        let extent: Vec<f64> = (0..3).map(|k| (hi[k] - lo[k]).max(0.0)).collect();
        let mut axes = [0usize, 1, 2];
        axes.sort_by(|a, b| extent[*b].total_cmp(&extent[*a]).then(a.cmp(b)));
        let diagonal = extent.iter().map(|e| e * e).sum::<f64>().sqrt();
        let diagonal = if diagonal > 0.0 { diagonal } else { 30.0 };
        SymbolLayout {
            hi,
            u: axes[0],
            v: axes[1],
            height: diagonal / 30.0,
            right: lo[axes[0]] - diagonal,
        }
    }

    /// The symbol's plane (origin, normal, direction in it) and its polylines: the label boxed,
    /// a stem down to a triangle whose apex is `tip`.
    #[allow(clippy::type_complexity)]
    fn draw(&mut self, tip: [f64; 3], label: &str) -> ([[f64; 3]; 3], Vec<Vec<[f64; 3]>>) {
        let (u, v, h) = (self.u, self.v, self.height);
        let at = |a: f64, b: f64| {
            let mut p = tip;
            p[u] = a;
            p[v] = b;
            p
        };
        let (strokes, width) = lettering(label, h);
        let size = 1.6 * h;
        let frame = size.max(width + 0.6 * h);
        let left = (tip[u] - frame / 2.0).max(self.right + size / 2.0);
        self.right = left + frame;
        let bottom = self.hi[v].max(tip[v]) + size;
        let mut lines = vec![vec![
            at(left, bottom),
            at(left + frame, bottom),
            at(left + frame, bottom + size),
            at(left, bottom + size),
            at(left, bottom),
        ]];
        let (a, b) = (left + (frame - width) / 2.0, bottom + (size - h) / 2.0);
        for s in &strokes {
            lines.push(s.iter().map(|(x, y)| at(a + x, b + y)).collect());
        }
        let half = h / 2.0;
        let (tu, tv) = (tip[u], tip[v]);
        let triangle = vec![
            at(tu - half, tv + half),
            at(tu + half, tv + half),
            at(tu, tv),
            at(tu - half, tv + half),
        ];
        lines.push(vec![at(left + frame / 2.0, bottom), at(tu, tv + half)]);
        lines.push(triangle);
        let mut normal = [0.0; 3];
        // e_u × e_v, toward the viewer.
        let w = 3 - u - v;
        normal[w] = if (u + 1) % 3 == v { 1.0 } else { -1.0 };
        let mut along = [0.0; 3];
        along[u] = 1.0;
        let round = |p: [f64; 3]| p.map(|x| (x * 1e6).round() / 1e6 + 0.0);
        let lines = lines
            .into_iter()
            .map(|l| l.into_iter().map(round).collect())
            .collect();
        ([round(at(left, bottom)), normal, along], lines)
    }
}

// ---------------------------------------------------------------------------------------------
// Semantic comparison
// ---------------------------------------------------------------------------------------------

/// The differences between two parts' PMI as meaning, not as indices: every reference is
/// resolved to what it names (a feature to its faces, edges and geometry; a datum to its label
/// and feature), collections are compared as sets (features of equal content are one feature;
/// modifiers in any order), and a one-datum compartment's own modifiers are the compartment's.
/// Values are compared as quantities (millimetres or radians, to 1e-12 relative) with their
/// decimal places; [`differences_as_stated`] compares their text and unit too. Empty when the
/// two mean the same.
#[must_use]
pub fn differences(a: &PartPmi, b: &PartPmi) -> Vec<String> {
    compare(a, b, false)
}

/// [`differences`], with every value compared as stated: its decimal text and its unit (a
/// round trip writes each value's own digits in its own unit).
#[must_use]
pub fn differences_as_stated(a: &PartPmi, b: &PartPmi) -> Vec<String> {
    compare(a, b, true)
}

fn compare(a: &PartPmi, b: &PartPmi, stated: bool) -> Vec<String> {
    let ka = Canonical::of(a, stated);
    let kb = Canonical::of(b, stated);
    let mut out = Vec::new();
    for (name, x, y) in [
        ("standard", &ka.standards, &kb.standards),
        ("decimal places", &ka.decimal_places, &kb.decimal_places),
        ("feature", &ka.features, &kb.features),
        ("datum target", &ka.datum_targets, &kb.datum_targets),
        ("datum", &ka.datums, &kb.datums),
        ("dimension", &ka.dimensions, &kb.dimensions),
        ("tolerance", &ka.tolerances, &kb.tolerances),
        ("tolerance relation", &ka.relations, &kb.relations),
        ("general tolerance", &ka.general, &kb.general),
        ("thread", &ka.threads, &kb.threads),
        ("knurl", &ka.knurls, &kb.knurls),
        ("material", &ka.material, &kb.material),
        ("note", &ka.notes, &kb.notes),
        (
            "surface texture",
            &ka.surface_textures,
            &kb.surface_textures,
        ),
        ("attribute", &ka.attributes, &kb.attributes),
    ] {
        let mut ys = y.clone();
        for k in x {
            match ys.iter().position(|o| o == k) {
                Some(i) => {
                    ys.remove(i);
                }
                None => out.push(format!("{name} only in the first: {k}")),
            }
        }
        for k in ys {
            out.push(format!("{name} only in the second: {k}"));
        }
    }
    out
}

/// A part's PMI with every index resolved, each collection sorted.
struct Canonical {
    standards: Vec<String>,
    decimal_places: Vec<String>,
    features: Vec<String>,
    datum_targets: Vec<String>,
    datums: Vec<String>,
    dimensions: Vec<String>,
    tolerances: Vec<String>,
    relations: Vec<String>,
    general: Vec<String>,
    threads: Vec<String>,
    knurls: Vec<String>,
    material: Vec<String>,
    notes: Vec<String>,
    surface_textures: Vec<String>,
    attributes: Vec<String>,
}

impl Canonical {
    fn of(p: &PartPmi, stated: bool) -> Canonical {
        let k = Keys { p, stated };
        let sorted = |mut v: Vec<String>| {
            v.sort();
            v
        };
        let mut features = sorted(p.features.iter().map(|f| k.feature_content(f)).collect());
        features.dedup();
        let ol = |l: &Option<Length>| l.as_ref().map(|l| k.length(l));
        Canonical {
            standards: sorted(p.standards.iter().map(|s| format!("{s:?}")).collect()),
            decimal_places: p.decimal_places.iter().map(|d| d.to_string()).collect(),
            features,
            datum_targets: sorted((0..p.datum_targets.len()).map(|i| k.target(i)).collect()),
            datums: sorted((0..p.datums.len()).map(|i| k.datum(i)).collect()),
            dimensions: sorted((0..p.dimensions.len()).map(|i| k.dimension(i)).collect()),
            tolerances: sorted((0..p.tolerances.len()).map(|i| k.tolerance(i)).collect()),
            relations: sorted(
                p.tolerance_relations
                    .iter()
                    .map(|r| {
                        format!(
                            "{:?} {} -> {}",
                            r.kind,
                            k.tolerance(r.relating.0),
                            k.tolerance(r.related.0)
                        )
                    })
                    .collect(),
            ),
            general: sorted(p.general.iter().map(|g| k.general(g)).collect()),
            threads: sorted(
                p.threads
                    .iter()
                    .map(|t| {
                        format!(
                            "{:?} major {} minor {:?} pitch diameter {:?} threads {} form {:?} fit {:?} {:?} {:?} crest {:?} qualifier {:?} nominal {:?} on {} area {:?} runout {:?}",
                            t.side,
                            k.length(&t.major_diameter),
                            ol(&t.minor_diameter),
                            ol(&t.pitch_diameter),
                            k.decimal(&t.number_of_threads.0),
                            t.form,
                            t.fit_class,
                            t.fit_class_2,
                            t.hand,
                            ol(&t.crest),
                            t.qualifier,
                            ol(&t.nominal_size),
                            k.feature(t.feature),
                            t.partial_area.map(|f| k.feature(f)),
                            t.runout.map(|f| k.feature(f))
                        )
                    })
                    .collect(),
            ),
            knurls: sorted(
                p.knurls
                    .iter()
                    .map(|x| {
                        format!(
                            "{:?} major {} nominal {} pitch {} teeth {:?} depth {:?} fillet {:?} helix {:?} {:?} on {}",
                            x.pattern,
                            k.length(&x.major_diameter),
                            k.length(&x.nominal_diameter),
                            k.length(&x.diametral_pitch),
                            x.number_of_teeth,
                            ol(&x.tooth_depth),
                            ol(&x.root_fillet),
                            x.helix_angle.as_ref().map(|a| k.angle(a)),
                            x.helix_hand,
                            k.feature(x.feature)
                        )
                    })
                    .collect(),
            ),
            material: p.material.iter().map(|m| format!("{m:?}")).collect(),
            notes: sorted(
                p.notes
                    .iter()
                    .map(|n| format!("{:?} {:?} on {}", n.kind, n.text, k.owner(n.on)))
                    .collect(),
            ),
            surface_textures: sorted(
                p.surface_textures
                    .iter()
                    .map(|s| {
                        let ps: Vec<String> = s
                            .parameters
                            .iter()
                            .map(|x| format!("{:?} {}", x.characteristic, k.length(&x.value)))
                            .collect();
                        format!(
                            "{:?} [{}] on {}",
                            s.material_removal,
                            ps.join(", "),
                            k.owner(s.on.map(NoteOwner::Feature))
                        )
                    })
                    .collect(),
            ),
            attributes: sorted(
                p.attributes
                    .iter()
                    .map(|a| {
                        let items: Vec<String> = a
                            .items
                            .iter()
                            .map(|(n, v)| {
                                let v = match v {
                                    AttributeValue::Measure(m) => k.value(m),
                                    AttributeValue::Real(d) => k.decimal(d),
                                    AttributeValue::OtherMeasure {
                                        measure,
                                        value,
                                        unit,
                                    } => format!("{measure} {} {unit:?}", k.decimal(value)),
                                    v => format!("{v:?}"),
                                };
                                format!("{n:?}={v}")
                            })
                            .collect();
                        format!("{:?} [{}] on {}", a.name, items.join(", "), k.owner(a.on))
                    })
                    .collect(),
            ),
        }
    }
}

struct Keys<'p> {
    p: &'p PartPmi,
    /// Values as stated (text and unit), else as quantities.
    stated: bool,
}

impl Keys<'_> {
    fn decimal(&self, d: &Decimal) -> String {
        if self.stated {
            d.to_string()
        } else {
            format!("{:.11e}", d.to_f64())
        }
    }

    fn length(&self, l: &Length) -> String {
        if self.stated {
            format!("{}{:?}", l.value, l.unit)
        } else {
            format!("{:.11e} mm", l.mm())
        }
    }

    fn angle(&self, a: &Angle) -> String {
        if self.stated {
            format!("{}{:?}", a.value, a.unit)
        } else {
            format!("{:.11e} rad", a.rad())
        }
    }

    fn value(&self, v: &Value) -> String {
        let q = match &v.quantity {
            Quantity::Length(l) => self.length(l),
            Quantity::Angle(a) => self.angle(a),
        };
        match v.decimal_places {
            Some(p) => format!("{q}/{p}"),
            None => q,
        }
    }

    fn ov(&self, v: &Option<Value>) -> String {
        v.as_ref()
            .map_or_else(|| "-".to_string(), |v| self.value(v))
    }

    fn bounds(&self, b: &Bounds) -> String {
        format!("{} {}", self.value(b.upper()), self.value(b.lower()))
    }

    fn general(&self, g: &GeneralTolerance) -> String {
        match g {
            GeneralTolerance::Class { text, standard } => format!("class {text:?} {standard:?}"),
            GeneralTolerance::Table { name, cells } => {
                let cells: Vec<String> = cells
                    .iter()
                    .map(|c| {
                        c.items
                            .iter()
                            .map(|(n, v)| {
                                let v = match v {
                                    CellValue::Value(x) => self.value(x),
                                    CellValue::Count(d) => self.decimal(d),
                                    CellValue::Text(t) => format!("{t:?}"),
                                };
                                format!("{n:?}={v}")
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .collect();
                format!("table {name:?} [{}]", cells.join("; "))
            }
        }
    }

    fn feature(&self, f: FeatureId) -> String {
        self.p.features.get(f.0).map_or_else(
            || format!("missing feature {}", f.0),
            |x| self.feature_content(x),
        )
    }

    fn feature_content(&self, f: &Feature) -> String {
        match f {
            Feature::Items(items) => {
                let mut v: Vec<String> = items
                    .iter()
                    .map(|a| match a {
                        Anchor::Face(x) => format!("face {}", x.0),
                        Anchor::Edge(x) => format!("edge {}", x.0),
                        Anchor::Geometry(g) => format!("geometry {:?}", self.p.geometry.get(g.0)),
                    })
                    .collect();
                v.sort();
                v.dedup();
                format!("[{}]", v.join(", "))
            }
            Feature::Group { members, kind } => {
                let mut v: Vec<String> = members.iter().map(|m| self.feature(*m)).collect();
                v.sort();
                format!("{kind:?}({})", v.join(", "))
            }
            Feature::Derived { kind, from } => {
                let mut v: Vec<String> = from.iter().map(|m| self.feature(*m)).collect();
                v.sort();
                format!("{kind:?}<-({})", v.join(", "))
            }
        }
    }

    fn target(&self, i: usize) -> String {
        let t = &self.p.datum_targets[i];
        let shape = match t.shape() {
            TargetShape::Area(f) => format!("Area({})", self.feature(*f)),
            TargetShape::Curve(f) => format!("Curve({})", self.feature(*f)),
            TargetShape::Point => "Point".into(),
            TargetShape::Line { length } => format!("Line {}", self.length(length)),
            TargetShape::Rectangle { length, width } => {
                format!("Rectangle {} {}", self.length(length), self.length(width))
            }
            TargetShape::Circle { diameter } => format!("Circle {}", self.length(diameter)),
            TargetShape::CircularCurve { diameter } => {
                format!("CircularCurve {}", self.length(diameter))
            }
        };
        let placement = t.placement().map(|pl| {
            format!(
                "at {} {} {} axis {:?} ref {:?}",
                self.length(&pl.origin[0]),
                self.length(&pl.origin[1]),
                self.length(&pl.origin[2]),
                pl.axis,
                pl.ref_direction
            )
        });
        format!(
            "{} {shape} {placement:?} {:?} on {:?}",
            t.number(),
            t.movable(),
            t.on().map(|f| self.feature(f))
        )
    }

    fn datum(&self, i: usize) -> String {
        let d = &self.p.datums[i];
        let mut ts: Vec<String> = d.targets().iter().map(|t| self.target(t.0)).collect();
        ts.sort();
        format!(
            "{} feature {:?} targets [{}]",
            d.label(),
            d.feature().map(|f| self.feature(f)),
            ts.join("; ")
        )
    }

    fn label(&self, d: DatumId) -> String {
        self.p.datums.get(d.0).map_or_else(
            || format!("missing datum {}", d.0),
            |x| x.label().to_string(),
        )
    }

    fn modifiers(&self, ms: &[DatumModifier]) -> String {
        let mut v: Vec<String> = ms
            .iter()
            .map(|m| match m {
                DatumModifier::Simple(s) => format!("{s:?}"),
                DatumModifier::WithValue { kind, value } => {
                    format!("{kind:?} {}", self.value(value))
                }
            })
            .collect();
        v.sort();
        v.join(",")
    }

    fn system(&self, ds: &DatumSystem) -> String {
        ds.compartments()
            .iter()
            .map(|c| match c.references.as_slice() {
                [one] => {
                    let mut m = c.modifiers.clone();
                    m.extend(one.modifiers.iter().cloned());
                    format!("{}[{}]", self.label(one.datum), self.modifiers(&m))
                }
                many => {
                    let mut v: Vec<String> = many
                        .iter()
                        .map(|r| {
                            format!("{}[{}]", self.label(r.datum), self.modifiers(&r.modifiers))
                        })
                        .collect();
                    v.sort();
                    format!("({})[{}]", v.join("-"), self.modifiers(&c.modifiers))
                }
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn dimension(&self, i: usize) -> String {
        let d = &self.p.dimensions[i];
        let kind = match &d.kind {
            DimensionKind::Size {
                feature,
                kind,
                path,
                angle,
            } => format!(
                "size {kind:?} of {} path {:?} angle {angle:?}",
                self.feature(*feature),
                path.map(|p| self.feature(p))
            ),
            DimensionKind::Location {
                from,
                to,
                kind,
                path,
                directed,
                angle,
            } => format!(
                "location {kind:?} from {} to {} path {:?} directed {directed} angle {angle:?}",
                self.feature(*from),
                self.feature(*to),
                path.map(|p| self.feature(p))
            ),
        };
        let tolerance = match &d.tolerance {
            DimTolerance::None => "none".to_string(),
            DimTolerance::Basic => "basic".to_string(),
            DimTolerance::Deviations(b) => format!("deviations {}", self.bounds(b)),
            DimTolerance::Limits(b) => format!("limits {}", self.bounds(b)),
            DimTolerance::Fit { class, limits } => format!(
                "fit {class} {}",
                limits.as_ref().map_or_else(String::new, |b| self.bounds(b))
            ),
        };
        let mut m: Vec<String> = d.modifiers.iter().map(|x| format!("{x:?}")).collect();
        m.sort();
        format!(
            "{kind} nominal {} {tolerance} qualifier {:?} modifiers {m:?} principle {:?}",
            self.ov(&d.nominal),
            d.qualifier,
            d.principle
        )
    }

    fn tolerance(&self, i: usize) -> String {
        let Some(t) = self.p.tolerances.get(i) else {
            return format!("missing tolerance {i}");
        };
        let target = match &t.target {
            ToleranceTarget::Feature(f) => format!("feature {}", self.feature(*f)),
            ToleranceTarget::Dimension(d) => format!("dimension {}", self.dimension(d.0)),
            ToleranceTarget::Relation {
                relating,
                related,
                name,
            } => format!(
                "relation {name:?} {} -> {}",
                self.feature(*relating),
                self.feature(*related)
            ),
            ToleranceTarget::WholePart => "whole part".into(),
        };
        let zone = t.zone.as_ref().map(|z| {
            let mut nu: Option<Vec<String>> = z
                .non_uniform
                .as_ref()
                .map(|v| v.iter().map(|f| self.feature(*f)).collect());
            if let Some(v) = &mut nu {
                v.sort();
            }
            format!(
                "{:?} projected {:?} non-uniform {nu:?} runout {} affected {:?}",
                z.form,
                z.projected
                    .as_ref()
                    .map(|p| (p.end.map(|f| self.feature(f)), self.value(&p.length))),
                self.ov(&z.runout_angle),
                z.affected_plane.map(|f| self.feature(f))
            )
        });
        let unit = t.unit_basis.as_ref().map(|u| {
            format!(
                "{} {:?}",
                self.value(&u.size),
                u.area.as_ref().map(|a| (a.shape, self.ov(&a.second)))
            )
        });
        let mut mods: Vec<String> = t.modifiers.iter().map(|m| format!("{m:?}")).collect();
        mods.sort();
        let mut aux: Vec<String> = t.auxiliary.iter().map(|m| format!("{m:?}")).collect();
        aux.sort();
        format!(
            "{:?} on {target} magnitude {} zone {zone:?} modifiers {mods:?} unit {unit:?} maximum {} unequal {} datums {:?} auxiliary {aux:?} description {:?}",
            t.kind,
            self.ov(&t.magnitude),
            self.ov(&t.maximum),
            self.ov(&t.unequal),
            t.datums.as_ref().map(|d| self.system(d)),
            t.description
        )
    }

    fn owner(&self, o: Option<NoteOwner>) -> String {
        match o {
            None => "part".into(),
            Some(NoteOwner::Feature(f)) => format!("feature {}", self.feature(f)),
            Some(NoteOwner::Dimension(d)) => format!("dimension {}", self.dimension(d.0)),
            Some(NoteOwner::Tolerance(t)) => format!("tolerance {}", self.tolerance(t.0)),
            Some(NoteOwner::DatumTarget(t)) => format!("datum target {}", self.target(t.0)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complex_leaves_are_sorted_whatever_the_order_given() {
        // The only way to make a complex record sorts its leaves: the external mapping's
        // order cannot be broken through this layer.
        let c = Complex::new([
            p21::leaf("REPRESENTATION_ITEM", vec![s("x")]),
            p21::leaf("LENGTH_MEASURE_WITH_UNIT", vec![]),
            p21::leaf("MEASURE_REPRESENTATION_ITEM", vec![]),
        ])
        .unwrap();
        let names: Vec<&str> = c.leaves().iter().map(|l| l.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "LENGTH_MEASURE_WITH_UNIT",
                "MEASURE_REPRESENTATION_ITEM",
                "REPRESENTATION_ITEM"
            ]
        );
    }

    #[test]
    fn a_value_that_is_not_a_part21_real_is_refused_not_rewritten() {
        for t in ["35", "1.e-3", "2E1"] {
            assert!(real(&Decimal::parse(t).unwrap()).is_err(), "{t}");
        }
        for t in ["35.", "0.0030", "-2.54E1", "1.E-07"] {
            assert!(real(&Decimal::parse(t).unwrap()).is_ok(), "{t}");
        }
    }

    #[test]
    fn deviations_cannot_be_given_as_a_magnitude() {
        // A lower deviation exists only as a signed value of `Bounds`, whose upper is above its
        // lower: "0.05 below" is -0.05, never a magnitude with an implied sign.
        let mm = |t: &str| Value::length(Decimal::parse(t).unwrap(), LengthUnit::Millimetre);
        assert!(Bounds::new(mm("0.1"), mm("0.1")).is_err());
        assert!(Bounds::new(mm("-0.025"), mm("-0.009")).is_err());
        let g6 = Bounds::new(mm("-0.009"), mm("-0.025")).unwrap();
        assert_eq!(g6.lower().decimal().as_str(), "-0.025");
    }

    #[test]
    fn zone_form_names_are_table_13s() {
        assert_eq!(
            zone_form_name(&ZoneForm::WithinACylinder),
            "within a cylinder"
        );
        assert_eq!(zone_form_name(&ZoneForm::Other(String::new())), "");
    }
}
