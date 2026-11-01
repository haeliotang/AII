//! AAS v3 helpers: identifier encoding, unit mapping, `xs:dateTime` parsing and
//! extraction of the pump Submodel properties from BaSyx JSON.

use poie_ontology::model::AssetStatus;
use serde_json::Value;

/// Base64url without padding, as used for identifiers in AAS API paths.
pub fn base64url(input: &str) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        let symbols = chunk.len() + 1;
        for index in 0..symbols {
            out.push(TABLE[((n >> (18 - 6 * index)) & 0x3f) as usize] as char);
        }
    }
    out
}

/// Converts a flow value with the given unit to SI (m3/s). Unknown units are
/// rejected rather than guessed.
pub fn flow_to_si(value: f64, unit: &str) -> Result<f64, String> {
    let factor = match unit.trim() {
        "m3/s" | "m³/s" => 1.0,
        "m3/h" | "m³/h" => 1.0 / 3600.0,
        "L/min" | "l/min" => 1.0 / 60_000.0,
        "L/s" | "l/s" => 1.0 / 1000.0,
        other => return Err(format!("unit {other:?} is not in the adapter's unit table")),
    };
    if !value.is_finite() {
        return Err(format!("non-finite value {value}"));
    }
    Ok(value * factor)
}

/// Parses an `xs:dateTime` in UTC (`Z` or `+00:00`) to Unix milliseconds.
/// Other offsets and missing time zones are rejected: without a zone the
/// instant is ambiguous and must not be used as event time.
pub fn parse_datetime_ms(text: &str) -> Result<i64, String> {
    let bad = || format!("unsupported xs:dateTime {text:?}");
    let text = text.trim();
    let (main, zone_ok) = if let Some(stripped) = text.strip_suffix('Z') {
        (stripped, true)
    } else if let Some(stripped) = text.strip_suffix("+00:00") {
        (stripped, true)
    } else {
        (text, false)
    };
    if !zone_ok {
        return Err(format!("{} (UTC zone required)", bad()));
    }
    let (date, time) = main.split_once('T').ok_or_else(bad)?;
    let date: Vec<&str> = date.split('-').collect();
    if date.len() != 3 {
        return Err(bad());
    }
    let year: i64 = date[0].parse().map_err(|_| bad())?;
    let month: i64 = date[1].parse().map_err(|_| bad())?;
    let day: i64 = date[2].parse().map_err(|_| bad())?;
    let (hms, fraction) = match time.split_once('.') {
        Some((hms, fraction)) => (hms, fraction),
        None => (time, ""),
    };
    let hms: Vec<&str> = hms.split(':').collect();
    if hms.len() != 3 {
        return Err(bad());
    }
    let hour: i64 = hms[0].parse().map_err(|_| bad())?;
    let minute: i64 = hms[1].parse().map_err(|_| bad())?;
    let second: i64 = hms[2].parse().map_err(|_| bad())?;
    if !(1..=12).contains(&month) || !(1..=31).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return Err(bad());
    }
    let millis: i64 = if fraction.is_empty() {
        0
    } else {
        if !fraction.chars().all(|c| c.is_ascii_digit()) {
            return Err(bad());
        }
        let padded = format!("{fraction:0<3}");
        padded[..3].parse().map_err(|_| bad())?
    };
    let days = days_from_civil(year, month, day);
    Ok(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

/// Days since 1970-01-01 (proleptic Gregorian), H. Hinnant's algorithm.
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

pub fn status_from_text(text: &str) -> Result<AssetStatus, String> {
    match text.trim().to_ascii_lowercase().as_str() {
        "available" => Ok(AssetStatus::Available),
        "degraded" => Ok(AssetStatus::Degraded),
        "failed" => Ok(AssetStatus::Failed),
        other => Err(format!("OperatingState {other:?} has no status mapping")),
    }
}

/// One numeric property read with its declared unit.
#[derive(Clone, Debug, PartialEq)]
pub struct UnitReading {
    pub id_short: String,
    pub raw_value: String,
    pub unit: Option<String>,
    pub si: Result<f64, String>,
}

/// What the adapter extracts from one Submodel response.
#[derive(Clone, Debug, PartialEq)]
pub struct SubmodelReading {
    pub status: AssetStatus,
    pub observed_at: Result<i64, String>,
    pub observed_at_text: Option<String>,
    pub units: Vec<UnitReading>,
    /// Keys of every Property object, to check which metadata BaSyx returns.
    pub property_keys: Vec<(String, Vec<String>)>,
}

fn iec61360_unit(element: &Value) -> Option<String> {
    element.get("embeddedDataSpecifications")?.as_array()?.iter().find_map(|spec| {
        let content = spec.get("dataSpecificationContent")?;
        content.get("unit")?.as_str().map(str::to_string)
    })
}

pub fn read_submodel(json: &Value) -> Result<SubmodelReading, String> {
    let elements = json
        .get("submodelElements")
        .and_then(Value::as_array)
        .ok_or_else(|| "submodel has no submodelElements array".to_string())?;
    let mut status = None;
    let mut observed_at_text = None;
    let mut units = Vec::new();
    let mut property_keys = Vec::new();
    for element in elements {
        if element.get("modelType").and_then(Value::as_str) != Some("Property") {
            continue;
        }
        let id_short = element.get("idShort").and_then(Value::as_str).unwrap_or_default().to_string();
        let value = element.get("value").and_then(Value::as_str).map(str::to_string);
        let keys = element.as_object().map(|object| object.keys().cloned().collect()).unwrap_or_default();
        property_keys.push((id_short.clone(), keys));
        match id_short.as_str() {
            "OperatingState" => {
                let text = value.ok_or_else(|| "OperatingState has no value".to_string())?;
                status = Some(status_from_text(&text)?);
            }
            "ObservedAt" => observed_at_text = value,
            _ => {
                let Some(unit) = iec61360_unit(element) else { continue };
                let raw_value = value.unwrap_or_default();
                let si = raw_value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| format!("value {raw_value:?} is not numeric"))
                    .and_then(|number| flow_to_si(number, &unit));
                units.push(UnitReading { id_short, raw_value, unit: Some(unit), si });
            }
        }
    }
    let status = status.ok_or_else(|| "submodel has no OperatingState property".to_string())?;
    let observed_at = match &observed_at_text {
        Some(text) => parse_datetime_ms(text),
        None => Err("no ObservedAt property".to_string()),
    };
    Ok(SubmodelReading { status, observed_at, observed_at_text, units, property_keys })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_matches_known_vectors() {
        assert_eq!(base64url(""), "");
        assert_eq!(base64url("f"), "Zg");
        assert_eq!(base64url("fo"), "Zm8");
        assert_eq!(base64url("foo"), "Zm9v");
        assert_eq!(base64url("foob"), "Zm9vYg");
        assert_eq!(base64url("\u{fb}\u{ff}"), "w7vDvw");
        assert_eq!(base64url("urn:x?"), "dXJuOng_");
    }

    #[test]
    fn units_convert_or_are_rejected() {
        assert!((flow_to_si(12.5, "m3/h").unwrap() - 12.5 / 3600.0).abs() < 1e-15);
        assert!((flow_to_si(60.0, "L/min").unwrap() - 0.001).abs() < 1e-15);
        assert!(flow_to_si(100.0, "bbl/d").is_err());
        assert!(flow_to_si(f64::NAN, "m3/h").is_err());
    }

    #[test]
    fn datetimes_parse_in_utc_only() {
        assert_eq!(parse_datetime_ms("1970-01-01T00:00:00Z").unwrap(), 0);
        assert_eq!(parse_datetime_ms("2026-11-01T08:30:15Z").unwrap(), 1_793_521_815_000);
        assert_eq!(parse_datetime_ms("2026-11-01T08:30:15.25+00:00").unwrap(), 1_793_521_815_250);
        assert_eq!(parse_datetime_ms("2000-02-29T23:59:59.999Z").unwrap(), 951_868_799_999);
        assert!(parse_datetime_ms("2026-11-01T08:30:15").is_err());
        assert!(parse_datetime_ms("2026-11-01T08:30:15+02:00").is_err());
        assert!(parse_datetime_ms("2026-13-01T08:30:15Z").is_err());
    }

    #[test]
    fn the_pump_submodel_is_read() {
        let template =
            include_str!("../../data/submodel.template.json").replace("__OBSERVED_AT__", "2026-11-01T08:00:00Z");
        let json: Value = serde_json::from_str(&template).unwrap();
        let reading = read_submodel(&json).unwrap();
        assert_eq!(reading.status, AssetStatus::Degraded);
        assert_eq!(reading.observed_at, Ok(parse_datetime_ms("2026-11-01T08:00:00Z").unwrap()));
        assert_eq!(reading.units.len(), 2);
        assert_eq!(reading.units[0].unit.as_deref(), Some("m3/h"));
        assert!((reading.units[0].si.clone().unwrap() - 12.5 / 3600.0).abs() < 1e-15);
        assert!(reading.units[1].si.is_err());

        let mut broken = json.clone();
        broken["submodelElements"][0]["value"] = Value::String("spinning".into());
        assert!(read_submodel(&broken).is_err());
    }
}
