#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! 跨语言 wire RFC v1 · C1/C2 样例回放（string mantissa）。

use std::fs;
use std::path::PathBuf;

use decimalx::{Decimal, Money};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

fn cross_lang_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cross-lang")
}

#[derive(Debug, serde::Deserialize)]
struct Manifest {
    samples: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
struct SampleEnvelope {
    #[serde(rename = "type")]
    type_name: String,
    payload: Value,
}

fn load_manifest() -> Manifest {
    let path = cross_lang_dir().join("manifest.json");
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    serde_json::from_str(&text).expect("manifest parse")
}

fn load_sample(name: &str) -> SampleEnvelope {
    let path = cross_lang_dir().join(name);
    let text =
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    serde_json::from_str(&text).expect("sample parse")
}

fn assert_string_mantissa_roundtrip<T>(payload: &Value)
where
    T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let decoded: T =
        serde_json::from_value(payload.clone()).expect("deserialize cross-lang sample");
    let re = serde_json::to_value(&decoded).expect("re-serialize");
    assert_eq!(payload, &re, "string mantissa roundtrip mismatch");
}

#[test]
fn cross_lang_manifest_samples_exist() {
    let manifest = load_manifest();
    assert_eq!(manifest.samples.len(), 3);
    for name in &manifest.samples {
        assert!(cross_lang_dir().join(name).is_file(), "missing sample {name}");
    }
}

#[test]
fn cross_lang_decimal_and_money_string_mantissa() {
    for name in ["decimal_basic.v1.json", "money_usd.v1.json"] {
        let env = load_sample(name);
        match env.type_name.as_str() {
            "Decimal" => assert_string_mantissa_roundtrip::<Decimal>(&env.payload),
            "Money" => assert_string_mantissa_roundtrip::<Money>(&env.payload),
            other => panic!("unexpected type {other} in {name}"),
        }
    }
}

#[test]
fn cross_lang_legacy_integer_mantissa_still_reads() {
    let legacy = r#"{"mantissa":100,"scale":2}"#;
    let d: Decimal = serde_json::from_str(legacy).unwrap();
    assert_eq!(d.mantissa(), 100);
    assert_eq!(serde_json::to_string(&d).unwrap(), r#"{"mantissa":"100","scale":2}"#);
}
