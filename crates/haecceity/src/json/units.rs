//! Document-level unit identities, independent of quantity or item ordering.
use super::*;
pub(super) fn empty() -> Value {
    json!({"length":[],"angle":[],"density":[],"other":[]})
}
pub(super) fn validate(units: &Value, used: &mut BTreeSet<String>) -> Result<()> {
    #[cfg(feature = "json-import")]
    schema::check(
        units,
        &pmi_schema()["properties"]["units"],
        pmi_schema(),
        "/units",
    )?;
    for kind in ["length", "angle", "density", "other"] {
        let mut values = BTreeSet::new();
        for unit in units[kind]
            .as_array()
            .ok_or_else(|| Error(format!("missing {kind} unit table")))?
        {
            let id = unit["id"]
                .as_str()
                .ok_or_else(|| Error("missing unit id".into()))?;
            if id.is_empty() || !used.insert(id.to_string()) {
                return Err(Error(format!("duplicate or empty item id {id:?}")));
            }
            if !values.insert(identity::canonical(&unit["value"])?) {
                return Err(Error(format!(
                    "duplicate {kind} unit definition: reuse one unit id"
                )));
            }
        }
    }
    Ok(())
}
pub(super) fn table(kind: &str) -> Option<&'static str> {
    match kind {
        "LengthUnit" => Some("length"),
        "AngleUnit" => Some("angle"),
        "DensityUnit" => Some("density"),
        "OtherUnit" => Some("other"),
        _ => None,
    }
}
