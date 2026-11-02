//! Validated, namespaced parameter-table registry for WX-1.
//!
//! This first implementation deliberately supports a small, explicit CSV
//! profile. It does not guess WMO/provider semantics or silently overwrite
//! existing definitions. JSON/XLSX/YAML adapters are planned separately.
#![forbid(unsafe_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

/// Immutable identity of an imported table package.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct TableId {
    pub namespace: String,
    pub table: String,
    pub version: String,
}

/// External code tuple, including optional centre/subcentre context.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ExternalCode {
    pub centre: Option<String>,
    pub subcentre: Option<String>,
    pub discipline: Option<String>,
    pub category: Option<String>,
    pub number: String,
}

/// One parameter definition. Source code identity is kept separate from name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameter {
    pub code: ExternalCode,
    pub canonical_id: String,
    pub short_name: String,
    pub long_name: String,
    pub units: String,
    pub description: String,
}

/// A table package as imported from the WX-1 CSV profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TablePackage {
    pub id: TableId,
    pub publisher: String,
    pub source: String,
    pub license: String,
    pub parameters: Vec<Parameter>,
}

/// Errors are explicit; invalid data is never partially installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    InvalidField(&'static str),
    MissingColumn(String),
    InvalidCsv(String),
    DuplicateCode(ExternalCode),
    DuplicateCanonicalId(String),
    TableAlreadyExists(TableId),
    TableNotFound(TableId),
    IncompatibleUnits {
        from: String,
        to: String,
        reason: &'static str,
    },
    UnknownUnit(String),
}

impl fmt::Display for RegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidField(s) => write!(f, "invalid or empty field: {s}"),
            Self::MissingColumn(s) => write!(f, "missing required CSV column: {s}"),
            Self::InvalidCsv(s) => write!(f, "invalid CSV: {s}"),
            Self::DuplicateCode(c) => write!(f, "duplicate external code: {}", c.number),
            Self::DuplicateCanonicalId(s) => write!(f, "duplicate canonical parameter ID: {s}"),
            Self::TableAlreadyExists(t) => write!(
                f,
                "table version already exists: {}:{}@{}",
                t.namespace, t.table, t.version
            ),
            Self::TableNotFound(t) => write!(
                f,
                "table not found: {}:{}@{}",
                t.namespace, t.table, t.version
            ),
            Self::IncompatibleUnits { from, to, reason } => {
                write!(f, "incompatible units '{from}' to '{to}': {reason}")
            }
            Self::UnknownUnit(u) => write!(f, "unknown physical unit: {u}"),
        }
    }
}
impl std::error::Error for RegistryError {}

impl TablePackage {
    /// Validate package identity and all entry uniqueness constraints.
    pub fn validate(&self) -> Result<(), RegistryError> {
        for (name, value) in [
            ("namespace", self.id.namespace.as_str()),
            ("table", self.id.table.as_str()),
            ("version", self.id.version.as_str()),
            ("publisher", self.publisher.as_str()),
            ("source", self.source.as_str()),
            ("license", self.license.as_str()),
        ] {
            if value.trim().is_empty() {
                return Err(RegistryError::InvalidField(name));
            }
        }
        let mut codes = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for p in &self.parameters {
            for (name, value) in [
                ("canonical_id", p.canonical_id.as_str()),
                ("short_name", p.short_name.as_str()),
                ("long_name", p.long_name.as_str()),
                ("units", p.units.as_str()),
            ] {
                if value.trim().is_empty() {
                    return Err(RegistryError::InvalidField(name));
                }
            }
            if p.code.number.trim().is_empty() {
                return Err(RegistryError::InvalidField("code.number"));
            }
            if !codes.insert(p.code.clone()) {
                return Err(RegistryError::DuplicateCode(p.code.clone()));
            }
            if !ids.insert(p.canonical_id.clone()) {
                return Err(RegistryError::DuplicateCanonicalId(p.canonical_id.clone()));
            }
        }
        Ok(())
    }
}

/// In-memory registry. Published table versions are immutable once inserted.
#[derive(Debug, Default)]
pub struct Registry {
    tables: BTreeMap<TableId, TablePackage>,
}
impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn insert(&mut self, package: TablePackage) -> Result<(), RegistryError> {
        package.validate()?;
        if self.tables.contains_key(&package.id) {
            return Err(RegistryError::TableAlreadyExists(package.id));
        }
        self.tables.insert(package.id.clone(), package);
        Ok(())
    }
    pub fn get(&self, id: &TableId) -> Option<&TablePackage> {
        self.tables.get(id)
    }
    pub fn resolve(&self, id: &TableId, code: &ExternalCode) -> Option<&Parameter> {
        self.tables
            .get(id)?
            .parameters
            .iter()
            .find(|p| &p.code == code)
    }
    pub fn table_count(&self) -> usize {
        self.tables.len()
    }

    /// Returns all installed table identities in deterministic order.
    pub fn table_ids(&self) -> Vec<&TableId> {
        self.tables.keys().collect()
    }

    /// Exports one installed table using the WX-1 CSV profile.
    pub fn export_csv(&self, id: &TableId) -> Result<String, RegistryError> {
        let package = self
            .tables
            .get(id)
            .ok_or_else(|| RegistryError::TableNotFound(id.clone()))?;
        export_csv(package)
    }
}

/// Classification of a parameter change between two versions of the same table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDiff {
    pub added: Vec<Parameter>,
    pub removed: Vec<Parameter>,
    pub changed: Vec<ParameterChange>,
}

/// A parameter whose definition differs between table versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterChange {
    pub before: Parameter,
    pub after: Parameter,
}

/// Compares two versions of the same namespace/table without guessing semantics.
/// Parameters are matched only by their complete external code tuple.
pub fn diff_tables(
    before: &TablePackage,
    after: &TablePackage,
) -> Result<TableDiff, RegistryError> {
    before.validate()?;
    after.validate()?;
    if before.id.namespace != after.id.namespace || before.id.table != after.id.table {
        return Err(RegistryError::InvalidField("diff table identity"));
    }
    let old: BTreeMap<ExternalCode, Parameter> = before
        .parameters
        .iter()
        .cloned()
        .map(|p| (p.code.clone(), p))
        .collect();
    let new: BTreeMap<ExternalCode, Parameter> = after
        .parameters
        .iter()
        .cloned()
        .map(|p| (p.code.clone(), p))
        .collect();
    let mut diff = TableDiff {
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
    };
    for (code, parameter) in &new {
        match old.get(code) {
            None => diff.added.push(parameter.clone()),
            Some(previous) if previous != parameter => diff.changed.push(ParameterChange {
                before: previous.clone(),
                after: parameter.clone(),
            }),
            Some(_) => {}
        }
    }
    for (code, parameter) in &old {
        if !new.contains_key(code) {
            diff.removed.push(parameter.clone());
        }
    }
    diff.added.sort_by_key(|a| a.code.clone());
    diff.removed.sort_by_key(|a| a.code.clone());
    diff.changed.sort_by_key(|a| a.after.code.clone());
    Ok(diff)
}

/// Serializes a validated package to deterministic CSV. Fields are emitted in
/// the documented profile order and quoted according to RFC-4180 rules.
pub fn export_csv(package: &TablePackage) -> Result<String, RegistryError> {
    package.validate()?;
    let mut output = String::from("code,canonical_id,short_name,long_name,units,description,centre,subcentre,discipline,category\r\n");
    let mut parameters: Vec<&Parameter> = package.parameters.iter().collect();
    parameters.sort_by_key(|a| a.code.clone());
    for p in parameters {
        let values = [
            p.code.number.as_str(),
            p.canonical_id.as_str(),
            p.short_name.as_str(),
            p.long_name.as_str(),
            p.units.as_str(),
            p.description.as_str(),
            p.code.centre.as_deref().unwrap_or(""),
            p.code.subcentre.as_deref().unwrap_or(""),
            p.code.discipline.as_deref().unwrap_or(""),
            p.code.category.as_deref().unwrap_or(""),
        ];
        for (index, value) in values.iter().enumerate() {
            if index > 0 {
                output.push(',');
            }
            write_csv_field(&mut output, value);
        }
        output.push_str("\r\n");
    }
    Ok(output)
}

fn write_csv_field(output: &mut String, value: &str) {
    if value
        .chars()
        .any(|ch| matches!(ch, ',' | '"' | '\r' | '\n'))
    {
        output.push('"');
        for ch in value.chars() {
            if ch == '"' {
                output.push_str("\"\"");
            } else {
                output.push(ch);
            }
        }
        output.push('"');
    } else {
        output.push_str(value);
    }
}

/// Import the initial WX-1 CSV profile. Required columns: `code`,
/// `canonical_id`, `short_name`, `long_name`, `units`. Optional columns:
/// `centre`, `subcentre`, `discipline`, `category`, `description`.
/// Package provenance is supplied separately and is never inferred from CSV.
pub fn import_csv(
    id: TableId,
    publisher: &str,
    source: &str,
    license: &str,
    input: &str,
) -> Result<TablePackage, RegistryError> {
    let mut records = parse_csv(input)?;
    if records.is_empty() {
        return Err(RegistryError::InvalidCsv("missing header".into()));
    }
    let headers: Vec<String> = records
        .remove(0)
        .into_iter()
        .map(|s| s.trim().to_owned())
        .collect();
    let mut positions = BTreeMap::new();
    for (i, h) in headers.iter().enumerate() {
        if positions.insert(h.as_str(), i).is_some() {
            return Err(RegistryError::InvalidCsv(format!("duplicate header {h}")));
        }
    }
    for required in ["code", "canonical_id", "short_name", "long_name", "units"] {
        if !positions.contains_key(required) {
            return Err(RegistryError::MissingColumn(required.into()));
        }
    }
    let get = |row: &[String], name: &str| -> String {
        positions
            .get(name)
            .and_then(|i| row.get(*i))
            .cloned()
            .unwrap_or_default()
            .trim()
            .to_owned()
    };
    let mut parameters = Vec::with_capacity(records.len());
    for (line, row) in records.into_iter().enumerate() {
        if row.len() != headers.len() {
            return Err(RegistryError::InvalidCsv(format!(
                "record {} has {} fields; expected {}",
                line + 2,
                row.len(),
                headers.len()
            )));
        }
        let optional = |name: &str| {
            let s = get(&row, name);
            if s.is_empty() {
                None
            } else {
                Some(s)
            }
        };
        parameters.push(Parameter {
            code: ExternalCode {
                centre: optional("centre"),
                subcentre: optional("subcentre"),
                discipline: optional("discipline"),
                category: optional("category"),
                number: get(&row, "code"),
            },
            canonical_id: get(&row, "canonical_id"),
            short_name: get(&row, "short_name"),
            long_name: get(&row, "long_name"),
            units: get(&row, "units"),
            description: get(&row, "description"),
        });
    }
    let package = TablePackage {
        id,
        publisher: publisher.into(),
        source: source.into(),
        license: license.into(),
        parameters,
    };
    package.validate()?;
    Ok(package)
}

// Small RFC-4180-compatible field parser: commas/newlines in quoted fields,
// escaped double quotes, CRLF and final records without newline are handled.
fn parse_csv(input: &str) -> Result<Vec<Vec<String>>, RegistryError> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut chars = input.chars().peekable();
    let mut quoted = false;
    let mut after_quote = false;
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => {
                    quoted = false;
                    after_quote = true;
                }
                _ => field.push(c),
            }
        } else if after_quote {
            match c {
                ',' => {
                    row.push(std::mem::take(&mut field));
                    after_quote = false;
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    after_quote = false;
                }
                '\r' => {}
                _ if c.is_whitespace() => {}
                _ => {
                    return Err(RegistryError::InvalidCsv(
                        "unexpected character after closing quote".into(),
                    ))
                }
            }
        } else {
            match c {
                '"' if field.is_empty() => quoted = true,
                '"' => {
                    return Err(RegistryError::InvalidCsv(
                        "quote inside unquoted field".into(),
                    ))
                }
                ',' => row.push(std::mem::take(&mut field)),
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                }
                '\r' => {}
                _ => field.push(c),
            }
        }
    }
    if quoted {
        return Err(RegistryError::InvalidCsv(
            "unterminated quoted field".into(),
        ));
    }
    if !field.is_empty() || !row.is_empty() || after_quote {
        row.push(field);
        rows.push(row);
    }
    if rows.last().is_some_and(|r| r.iter().all(String::is_empty)) {
        rows.pop();
    }
    Ok(rows)
}

/// An explicit, separately versioned override package. It can replace existing
/// definitions only; it cannot silently add source codes or mutate the base.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableOverlay {
    pub base: TableId,
    pub target_version: String,
    pub publisher: String,
    pub source: String,
    pub license: String,
    pub reason: String,
    pub overrides: Vec<Parameter>,
}

/// Physical dimension for the meteorological unit vocabulary.
/// Unknown unit strings remain valid in ordinary table packages, but cannot be
/// used for an override whose dimensional compatibility cannot be verified.
pub fn unit_dimension(unit: &str) -> Option<&'static str> {
    match unit.trim() {
        "1" | "dimensionless" | "%" | "percent" | "fraction" => Some("dimensionless"),
        "K" | "kelvin" | "degC" | "degree_Celsius" | "°C" => Some("temperature"),
        "Pa" | "pascal" | "hPa" | "mbar" | "bar" => Some("pressure"),
        "m" | "meter" | "metre" | "km" | "mm" => Some("length"),
        "s" | "second" | "min" | "minute" | "h" | "hour" | "d" | "day" => Some("time"),
        "m/s" | "m s-1" | "m s^-1" | "km/h" | "knots" | "knot" => Some("velocity"),
        "kg/kg" | "kg kg-1" | "g/kg" => Some("mass_fraction"),
        "m2/s2" | "m^2 s^-2" | "J/kg" | "J kg-1" => Some("specific_energy"),
        "kg/m2" | "kg m-2" | "mm water equivalent" => Some("mass_per_area"),
        "W/m2" | "W m-2" => Some("energy_flux"),
        "s-1" | "s^-1" => Some("angular_velocity"),
        "kg/m3" | "kg m-3" => Some("density"),
        "kg m-2 s-1" | "kg/m2/s" | "mm/s" | "mm/h" => Some("precipitation_rate"),
        _ => None,
    }
}

/// Computes linear conversion coefficients `(scale, offset)` such that:
/// `value_to = (value_from * scale) + offset`.
/// Returns an error if units belong to different physical dimensions or are unknown.
pub fn unit_conversion_factor(from: &str, to: &str) -> Result<(f64, f64), RegistryError> {
    let dim_from =
        unit_dimension(from).ok_or_else(|| RegistryError::UnknownUnit(from.to_owned()))?;
    let dim_to = unit_dimension(to).ok_or_else(|| RegistryError::UnknownUnit(to.to_owned()))?;
    if dim_from != dim_to {
        return Err(RegistryError::IncompatibleUnits {
            from: from.to_owned(),
            to: to.to_owned(),
            reason: "units belong to different physical dimensions",
        });
    }

    // Convert from source to standard SI unit: (si = from * s1 + o1)
    let (s1, o1) = to_si_coefficients(from.trim(), dim_from)?;
    // Convert from target to standard SI unit: (si = to * s2 + o2) -> to = (si - o2) / s2
    let (s2, o2) = to_si_coefficients(to.trim(), dim_to)?;

    // value_to = ((value_from * s1 + o1) - o2) / s2 = value_from * (s1 / s2) + (o1 - o2) / s2
    let scale = s1 / s2;
    let offset = (o1 - o2) / s2;
    Ok((scale, offset))
}

fn to_si_coefficients(unit: &str, dimension: &str) -> Result<(f64, f64), RegistryError> {
    match dimension {
        "temperature" => match unit {
            "K" | "kelvin" => Ok((1.0, 0.0)),
            "degC" | "degree_Celsius" | "°C" => Ok((1.0, 273.15)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "pressure" => match unit {
            "Pa" | "pascal" => Ok((1.0, 0.0)),
            "hPa" | "mbar" => Ok((100.0, 0.0)),
            "bar" => Ok((100_000.0, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "length" => match unit {
            "m" | "meter" | "metre" => Ok((1.0, 0.0)),
            "km" => Ok((1_000.0, 0.0)),
            "mm" => Ok((0.001, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "time" => match unit {
            "s" | "second" => Ok((1.0, 0.0)),
            "min" | "minute" => Ok((60.0, 0.0)),
            "h" | "hour" => Ok((3_600.0, 0.0)),
            "d" | "day" => Ok((86_400.0, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "velocity" => match unit {
            "m/s" | "m s-1" | "m s^-1" => Ok((1.0, 0.0)),
            "km/h" => Ok((1.0 / 3.6, 0.0)),
            "knots" | "knot" => Ok((0.514_444_444_444_444_5, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "mass_fraction" => match unit {
            "kg/kg" | "kg kg-1" => Ok((1.0, 0.0)),
            "g/kg" => Ok((0.001, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "dimensionless" => match unit {
            "1" | "dimensionless" | "fraction" => Ok((1.0, 0.0)),
            "%" | "percent" => Ok((0.01, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "specific_energy" => match unit {
            "m2/s2" | "m^2 s^-2" | "J/kg" | "J kg-1" => Ok((1.0, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "mass_per_area" => match unit {
            "kg/m2" | "kg m-2" | "mm water equivalent" => Ok((1.0, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "energy_flux" => match unit {
            "W/m2" | "W m-2" => Ok((1.0, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "angular_velocity" => match unit {
            "s-1" | "s^-1" => Ok((1.0, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "density" => match unit {
            "kg/m3" | "kg m-3" => Ok((1.0, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        "precipitation_rate" => match unit {
            "kg m-2 s-1" | "kg/m2/s" | "mm/s" => Ok((1.0, 0.0)),
            "mm/h" => Ok((1.0 / 3600.0, 0.0)),
            _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
        },
        _ => Err(RegistryError::UnknownUnit(unit.to_owned())),
    }
}

/// Apply an explicit overlay and return a new immutable table version.
/// Every override must target an existing complete external code key and keep
/// its physical unit dimension. The base package is never modified.
pub fn apply_overlay(
    base: &TablePackage,
    overlay: &TableOverlay,
) -> Result<TablePackage, RegistryError> {
    base.validate()?;
    if overlay.base != base.id {
        return Err(RegistryError::InvalidField("overlay base identity"));
    }
    for (field, value) in [
        ("overlay.target_version", overlay.target_version.as_str()),
        ("overlay.publisher", overlay.publisher.as_str()),
        ("overlay.source", overlay.source.as_str()),
        ("overlay.license", overlay.license.as_str()),
        ("overlay.reason", overlay.reason.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(RegistryError::InvalidField(field));
        }
    }
    if overlay.target_version == base.id.version {
        return Err(RegistryError::InvalidField(
            "overlay target version must differ from base",
        ));
    }
    let mut result = base.clone();
    result.id.version = overlay.target_version.clone();
    result.publisher = overlay.publisher.clone();
    result.source = overlay.source.clone();
    result.license = overlay.license.clone();
    let mut seen = BTreeSet::new();
    for replacement in &overlay.overrides {
        if !seen.insert(replacement.code.clone()) {
            return Err(RegistryError::DuplicateCode(replacement.code.clone()));
        }
        let Some(index) = result
            .parameters
            .iter()
            .position(|p| p.code == replacement.code)
        else {
            return Err(RegistryError::InvalidField(
                "overlay cannot introduce a new external code",
            ));
        };
        let old_dimension = unit_dimension(&result.parameters[index].units);
        let new_dimension = unit_dimension(&replacement.units);
        if old_dimension.is_none() || new_dimension.is_none() || old_dimension != new_dimension {
            return Err(RegistryError::InvalidField(
                "overlay unit dimension is unknown or incompatible",
            ));
        }
        result.parameters[index] = replacement.clone();
    }
    result.validate()?;
    Ok(result)
}

/// Standard built-in table package provider.
pub fn standard_wmo_grib2_table() -> TablePackage {
    TablePackage {
        id: TableId {
            namespace: "wmo.grib2".into(),
            table: "4.2".into(),
            version: "2025".into(),
        },
        publisher: "World Meteorological Organization".into(),
        source: "WMO Manual on Codes, Volume I.2, Code Table 4.2".into(),
        license: "WMO Open Data Policy".into(),
        parameters: vec![
            Parameter {
                code: ExternalCode {
                    centre: None,
                    subcentre: None,
                    discipline: Some("0".into()),
                    category: Some("0".into()),
                    number: "0".into(),
                },
                canonical_id: "air_temperature".into(),
                short_name: "t".into(),
                long_name: "Air temperature".into(),
                units: "K".into(),
                description: "Temperature of air at specified level".into(),
            },
            Parameter {
                code: ExternalCode {
                    centre: None,
                    subcentre: None,
                    discipline: Some("0".into()),
                    category: Some("0".into()),
                    number: "6".into(),
                },
                canonical_id: "dew_point_temperature".into(),
                short_name: "td".into(),
                long_name: "Dewpoint temperature".into(),
                units: "K".into(),
                description: "Dewpoint temperature of air at specified level".into(),
            },
            Parameter {
                code: ExternalCode {
                    centre: None,
                    subcentre: None,
                    discipline: Some("0".into()),
                    category: Some("3".into()),
                    number: "0".into(),
                },
                canonical_id: "surface_pressure".into(),
                short_name: "sp".into(),
                long_name: "Pressure".into(),
                units: "Pa".into(),
                description: "Surface or atmospheric air pressure".into(),
            },
            Parameter {
                code: ExternalCode {
                    centre: None,
                    subcentre: None,
                    discipline: Some("0".into()),
                    category: Some("3".into()),
                    number: "5".into(),
                },
                canonical_id: "geopotential_height".into(),
                short_name: "gh".into(),
                long_name: "Geopotential height".into(),
                units: "gpm".into(),
                description: "Geopotential height above ground or sea level".into(),
            },
            Parameter {
                code: ExternalCode {
                    centre: None,
                    subcentre: None,
                    discipline: Some("0".into()),
                    category: Some("2".into()),
                    number: "2".into(),
                },
                canonical_id: "eastward_wind".into(),
                short_name: "u".into(),
                long_name: "U component of wind".into(),
                units: "m/s".into(),
                description: "Zonal eastward wind velocity component".into(),
            },
            Parameter {
                code: ExternalCode {
                    centre: None,
                    subcentre: None,
                    discipline: Some("0".into()),
                    category: Some("2".into()),
                    number: "3".into(),
                },
                canonical_id: "northward_wind".into(),
                short_name: "v".into(),
                long_name: "V component of wind".into(),
                units: "m/s".into(),
                description: "Meridional northward wind velocity component".into(),
            },
            Parameter {
                code: ExternalCode {
                    centre: None,
                    subcentre: None,
                    discipline: Some("0".into()),
                    category: Some("1".into()),
                    number: "8".into(),
                },
                canonical_id: "total_precipitation".into(),
                short_name: "tp".into(),
                long_name: "Total precipitation".into(),
                units: "kg/m2".into(),
                description: "Total precipitation accumulation".into(),
            },
        ],
    }
}

/// CF Standard Name mapping entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CfStandardNameMapping {
    pub standard_name: String,
    pub canonical_id: String,
    pub canonical_units: String,
    pub physical_dimension: &'static str,
}

/// Returns standard CF mappings for common meteorological variables.
pub fn standard_cf_mappings() -> Vec<CfStandardNameMapping> {
    vec![
        CfStandardNameMapping {
            standard_name: "air_temperature".into(),
            canonical_id: "air_temperature".into(),
            canonical_units: "K".into(),
            physical_dimension: "temperature",
        },
        CfStandardNameMapping {
            standard_name: "dew_point_temperature".into(),
            canonical_id: "dew_point_temperature".into(),
            canonical_units: "K".into(),
            physical_dimension: "temperature",
        },
        CfStandardNameMapping {
            standard_name: "surface_air_pressure".into(),
            canonical_id: "surface_pressure".into(),
            canonical_units: "Pa".into(),
            physical_dimension: "pressure",
        },
        CfStandardNameMapping {
            standard_name: "air_pressure".into(),
            canonical_id: "air_pressure".into(),
            canonical_units: "Pa".into(),
            physical_dimension: "pressure",
        },
        CfStandardNameMapping {
            standard_name: "geopotential_height".into(),
            canonical_id: "geopotential_height".into(),
            canonical_units: "m".into(),
            physical_dimension: "length",
        },
        CfStandardNameMapping {
            standard_name: "eastward_wind".into(),
            canonical_id: "eastward_wind".into(),
            canonical_units: "m s-1".into(),
            physical_dimension: "velocity",
        },
        CfStandardNameMapping {
            standard_name: "northward_wind".into(),
            canonical_id: "northward_wind".into(),
            canonical_units: "m s-1".into(),
            physical_dimension: "velocity",
        },
        CfStandardNameMapping {
            standard_name: "precipitation_amount".into(),
            canonical_id: "total_precipitation".into(),
            canonical_units: "kg m-2".into(),
            physical_dimension: "mass_per_area",
        },
    ]
}

/// Resolves a CF `standard_name` to its canonical metadata definition.
pub fn resolve_cf_standard_name(standard_name: &str) -> Option<CfStandardNameMapping> {
    standard_cf_mappings()
        .into_iter()
        .find(|m| m.standard_name == standard_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn id() -> TableId {
        TableId {
            namespace: "wmo.grib2".into(),
            table: "4.2".into(),
            version: "2025".into(),
        }
    }
    const CSV: &str = "code,canonical_id,short_name,long_name,units,description\n0,air_temperature,t,Air temperature,K,\"Temperature, near surface\"\n1,dew_point,td,dew point,K,dewpoint";
    #[test]
    fn imports_csv_and_preserves_quoted_commas() {
        let p = import_csv(id(), "WMO", "source", "license", CSV).unwrap();
        assert_eq!(p.parameters[0].description, "Temperature, near surface");
    }
    #[test]
    fn rejects_duplicate_code() {
        let csv = CSV.replace("1,dew_point", "0,dew_point");
        assert!(matches!(
            import_csv(id(), "WMO", "s", "l", &csv),
            Err(RegistryError::DuplicateCode(_))
        ));
    }
    #[test]
    fn rejects_missing_required_column() {
        assert!(matches!(
            import_csv(id(), "WMO", "s", "l", "code,short_name\n1,t"),
            Err(RegistryError::MissingColumn(_))
        ));
    }
    #[test]
    fn registry_refuses_table_version_overwrite() {
        let p = import_csv(id(), "WMO", "s", "l", CSV).unwrap();
        let mut r = Registry::new();
        r.insert(p.clone()).unwrap();
        assert!(matches!(
            r.insert(p),
            Err(RegistryError::TableAlreadyExists(_))
        ));
    }
    #[test]
    fn resolves_full_external_code_context() {
        let p = import_csv(id(), "WMO", "s", "l", CSV).unwrap();
        let code = p.parameters[0].code.clone();
        let mut r = Registry::new();
        r.insert(p).unwrap();
        assert_eq!(
            r.resolve(&id(), &code).unwrap().canonical_id,
            "air_temperature"
        );
    }
    #[test]
    fn rejects_unterminated_csv_quote() {
        assert!(matches!(
            import_csv(
                id(),
                "WMO",
                "s",
                "l",
                "code,canonical_id,short_name,long_name,units\n1,x,t,\"bad,K"
            ),
            Err(RegistryError::InvalidCsv(_))
        ));
    }
    #[test]
    fn exports_csv_with_escaped_fields_and_round_trips() {
        let package = import_csv(id(), "WMO", "source", "license", CSV).unwrap();
        let csv = export_csv(&package).unwrap();
        assert!(csv.contains("\"Temperature, near surface\""));
        let restored = import_csv(id(), "WMO", "source", "license", &csv).unwrap();
        assert_eq!(restored.parameters, package.parameters);
    }

    #[test]
    fn computes_added_removed_and_changed_parameters() {
        let before = import_csv(id(), "WMO", "source", "license", CSV).unwrap();
        let newer_id = TableId {
            version: "2026".into(),
            ..id()
        };
        let newer_csv = "code,canonical_id,short_name,long_name,units,description\n0,air_temperature,t2m,2m air temperature,K,updated\n2,wind_speed,ws,Wind speed,m/s,wind";
        let after = import_csv(newer_id, "WMO", "source", "license", newer_csv).unwrap();
        let diff = diff_tables(&before, &after).unwrap();
        assert_eq!(diff.added.len(), 1);
        assert_eq!(diff.removed.len(), 1);
        assert_eq!(diff.changed.len(), 1);
        assert_eq!(diff.added[0].canonical_id, "wind_speed");
        assert_eq!(diff.removed[0].canonical_id, "dew_point");
    }

    #[test]
    fn diff_rejects_different_table_families() {
        let before = import_csv(id(), "WMO", "source", "license", CSV).unwrap();
        let other_id = TableId {
            table: "4.3".into(),
            ..id()
        };
        let after = import_csv(other_id, "WMO", "source", "license", CSV).unwrap();
        assert!(matches!(
            diff_tables(&before, &after),
            Err(RegistryError::InvalidField(_))
        ));
    }

    #[test]
    fn overlay_creates_new_version_without_mutating_base() {
        let base = import_csv(id(), "WMO", "source", "license", CSV).unwrap();
        let mut replacement = base.parameters[0].clone();
        replacement.short_name = "t2m".into();
        replacement.long_name = "2 metre temperature".into();
        replacement.units = "degC".into();
        let overlay = TableOverlay {
            base: base.id.clone(),
            target_version: "local-1".into(),
            publisher: "Local operator".into(),
            source: "local correction".into(),
            license: "CC0-1.0".into(),
            reason: "Correct display metadata".into(),
            overrides: vec![replacement],
        };
        let updated = apply_overlay(&base, &overlay).unwrap();
        assert_eq!(updated.id.version, "local-1");
        assert_eq!(updated.parameters[0].short_name, "t2m");
        assert_eq!(base.parameters[0].short_name, "t");
    }

    #[test]
    fn overlay_rejects_dimension_change() {
        let base = import_csv(id(), "WMO", "source", "license", CSV).unwrap();
        let mut replacement = base.parameters[0].clone();
        replacement.units = "hPa".into();
        let overlay = TableOverlay {
            base: base.id.clone(),
            target_version: "local-1".into(),
            publisher: "Local operator".into(),
            source: "local correction".into(),
            license: "CC0-1.0".into(),
            reason: "test".into(),
            overrides: vec![replacement],
        };
        assert!(matches!(
            apply_overlay(&base, &overlay),
            Err(RegistryError::InvalidField(
                "overlay unit dimension is unknown or incompatible"
            ))
        ));
    }

    #[test]
    fn overlay_cannot_add_external_codes() {
        let base = import_csv(id(), "WMO", "source", "license", CSV).unwrap();
        let mut addition = base.parameters[0].clone();
        addition.code.number = "99".into();
        let overlay = TableOverlay {
            base: base.id.clone(),
            target_version: "local-1".into(),
            publisher: "Local operator".into(),
            source: "local correction".into(),
            license: "CC0-1.0".into(),
            reason: "test".into(),
            overrides: vec![addition],
        };
        assert!(matches!(
            apply_overlay(&base, &overlay),
            Err(RegistryError::InvalidField(
                "overlay cannot introduce a new external code"
            ))
        ));
    }

    #[test]
    fn unit_conversions_calculate_correct_linear_factors() {
        // Temperature: degC to K
        let (scale, offset) = unit_conversion_factor("degC", "K").unwrap();
        assert!((scale - 1.0).abs() < 1e-9);
        assert!((offset - 273.15).abs() < 1e-9);
        let k = 20.0 * scale + offset;
        assert!((k - 293.15).abs() < 1e-9);

        // Temperature: K to degC
        let (scale_rev, offset_rev) = unit_conversion_factor("K", "degC").unwrap();
        assert!((scale_rev - 1.0).abs() < 1e-9);
        assert!((offset_rev - (-273.15)).abs() < 1e-9);

        // Pressure: hPa to Pa
        let (scale_p, offset_p) = unit_conversion_factor("hPa", "Pa").unwrap();
        assert!((scale_p - 100.0).abs() < 1e-9);
        assert!((offset_p - 0.0).abs() < 1e-9);

        // Velocity: km/h to m/s
        let (scale_v, offset_v) = unit_conversion_factor("km/h", "m/s").unwrap();
        assert!((scale_v - (1.0 / 3.6)).abs() < 1e-9);
        assert!((offset_v - 0.0).abs() < 1e-9);
        let ms = 72.0 * scale_v + offset_v;
        assert!((ms - 20.0).abs() < 1e-9);

        // Dimension mismatch
        let err = unit_conversion_factor("hPa", "K").unwrap_err();
        assert!(matches!(err, RegistryError::IncompatibleUnits { .. }));

        // Unknown unit
        let err_unk = unit_conversion_factor("non_existent_unit", "K").unwrap_err();
        assert!(matches!(err_unk, RegistryError::UnknownUnit(_)));
    }

    #[test]
    fn standard_wmo_and_cf_tables_are_valid() {
        let wmo = standard_wmo_grib2_table();
        wmo.validate().unwrap();
        assert_eq!(wmo.parameters.len(), 7);

        let cf = resolve_cf_standard_name("air_temperature").unwrap();
        assert_eq!(cf.canonical_id, "air_temperature");
        assert_eq!(cf.canonical_units, "K");
        assert_eq!(cf.physical_dimension, "temperature");
    }
}
