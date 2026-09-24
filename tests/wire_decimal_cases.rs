#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! decimalx wire 黄金样本（tests/fixtures/wire/cases.v1.json）。
//!
//! 锁定 §13.2：`{mantissa, scale}`、MAX_SCALE、未知字段拒绝、Display 规范化。

use std::fs;
use std::path::PathBuf;
use std::str::FromStr;

use decimalx::{Decimal, MAX_SCALE, WIRE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn cases_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/wire/cases.v1.json")
}

#[derive(Debug, serde::Deserialize)]
struct CasesFile {
    cases: Vec<Case>,
}

#[derive(Debug, serde::Deserialize)]
struct Case {
    name: String,
    #[serde(default)]
    input: Option<String>,
    #[serde(default)]
    construct: Option<WireFields>,
    #[serde(default)]
    wire: Option<WireFields>,
    #[serde(default)]
    wire_input: Option<WireFields>,
    #[serde(default)]
    normalized_wire: Option<WireFields>,
    #[serde(default)]
    raw_json: Option<String>,
    #[serde(default)]
    display: Option<String>,
    #[serde(default)]
    expect_error: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct WireFields {
    mantissa: i128,
    scale: u8,
}

impl<'de> Deserialize<'de> for WireFields {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let v = Value::deserialize(deserializer)?;
        let obj = v.as_object().ok_or_else(|| serde::de::Error::custom("wire object"))?;
        let mantissa = match obj.get("mantissa") {
            Some(Value::String(s)) => s.parse().map_err(serde::de::Error::custom)?,
            Some(Value::Number(n)) => {
                n.as_i64().ok_or_else(|| serde::de::Error::custom("mantissa integer"))? as i128
            }
            _ => return Err(serde::de::Error::custom("mantissa")),
        };
        let scale = obj
            .get("scale")
            .and_then(|s| s.as_u64())
            .ok_or_else(|| serde::de::Error::custom("scale"))? as u8;
        Ok(Self { mantissa, scale })
    }
}

fn load_cases() -> CasesFile {
    let path = cases_path();
    let text = fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!("missing {}: {e}", path.display());
    });
    serde_json::from_str(&text).expect("cases.v1.json parse")
}

fn from_wire(w: &WireFields) -> Decimal {
    Decimal::try_new(w.mantissa, w.scale).unwrap_or_else(|e| {
        panic!("try_new({}, {}) failed: {e}", w.mantissa, w.scale);
    })
}

fn to_wire(d: Decimal) -> WireFields {
    let v = serde_json::to_value(d).expect("serialize Decimal");
    serde_json::from_value(v).expect("wire fields")
}

#[test]
fn wire_schema_version_is_v1() {
    assert_eq!(WIRE_SCHEMA_VERSION, 1);
    assert_eq!(MAX_SCALE, 18);
}

#[test]
fn table_driven_cases() {
    let file = load_cases();
    assert!(!file.cases.is_empty());

    for case in &file.cases {
        run_case(case);
    }
}

fn run_case(case: &Case) {
    let name = case.name.as_str();

    if case.expect_error {
        if let Some(raw) = &case.raw_json {
            let err = serde_json::from_str::<Decimal>(raw).unwrap_err();
            assert!(!err.to_string().is_empty(), "case {name}: expected deserialize error");
            return;
        }
        if let Some(w) = &case.wire_input {
            let v = serde_json::to_value(w).unwrap();
            assert!(
                serde_json::from_value::<Decimal>(v).is_err(),
                "case {name}: expected wire_input reject"
            );
            return;
        }
        if let Some(input) = &case.input {
            assert!(
                Decimal::from_str(input).is_err(),
                "case {name}: expected parse reject for {input:?}"
            );
            return;
        }
        panic!("case {name}: expect_error but no error vector");
    }

    let mut value: Option<Decimal> = None;

    if let Some(input) = &case.input {
        value = Some(Decimal::from_str(input).unwrap_or_else(|e| {
            panic!("case {name}: parse {input:?}: {e}");
        }));
    }
    if let Some(c) = &case.construct {
        value = Some(from_wire(c));
    }
    if let Some(w) = &case.wire
        && value.is_none()
    {
        value = Some(from_wire(w));
    }

    let Some(d) = value else {
        // normalize_equals 类：仅 construct + normalized_wire
        if let (Some(c), Some(n)) = (&case.construct, &case.normalized_wire) {
            let a = from_wire(c);
            let b = from_wire(n);
            assert_eq!(a, b, "case {name}: semantic eq after normalize pair");
            assert_eq!(a.normalize(), b, "case {name}: normalize()");
            return;
        }
        panic!("case {name}: no value vector");
    };

    if let Some(w) = &case.wire {
        assert_eq!(to_wire(d), *w, "case {name}: wire shape");
        let back: Decimal = serde_json::from_value(serde_json::to_value(w).unwrap()).unwrap();
        assert_eq!(back, d, "case {name}: wire deserialize");
    }

    if let Some(n) = &case.normalized_wire {
        let norm = d.normalize();
        assert_eq!(to_wire(norm), *n, "case {name}: normalized_wire");
        assert_eq!(norm, from_wire(n), "case {name}: normalize eq");
    }

    if let Some(disp) = &case.display {
        assert_eq!(d.to_string(), *disp, "case {name}: display");
    }
}

#[test]
fn roundtrip_json_value_eq() {
    let d = Decimal::try_new(6543210, 2).unwrap();
    let json = serde_json::to_string(&d).unwrap();
    assert_eq!(json, r#"{"mantissa":"6543210","scale":2}"#);
    let back: Decimal = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);
}

#[test]
fn unknown_field_rejected_smoke() {
    let j = r#"{"mantissa":1,"scale":0,"extra":1}"#;
    assert!(serde_json::from_str::<Decimal>(j).is_err());
}

#[test]
fn cases_file_is_valid_json_object() {
    let text = fs::read_to_string(cases_path()).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert!(v.get("cases").and_then(|c| c.as_array()).is_some());
}
