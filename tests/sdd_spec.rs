#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! `decimalx` 标准章节对应的可执行规格检查。
//!
//! // SPEC-MAP: D-1 | 职责 | assert_duties
//! // SPEC-MAP: D-2 | 范围 | assert_scope
//! // SPEC-MAP: D-3 | 约束 | assert_constraints

const STANDARD: &str = include_str!("../docs/标准.md");

#[test]
fn assert_duties() {
    assert!(STANDARD.contains("## 职责"));
    assert!(STANDARD.contains("精确十进制"));
}

#[test]
fn assert_scope() {
    assert!(STANDARD.contains("## 范围"));
    assert!(STANDARD.contains("Decimal"));
    assert_eq!(decimalx::MAX_SCALE, 18);
    assert!(decimalx::Decimal::try_new(1, decimalx::MAX_SCALE).is_ok());
    assert!(decimalx::Decimal::try_new(1, decimalx::MAX_SCALE + 1).is_err());
}

#[test]
fn assert_constraints() {
    assert!(STANDARD.contains("## 约束"));
    assert!(STANDARD.contains("checked_"));
    let overflow = decimalx::Decimal::MAX
        .checked_add(decimalx::Decimal::ONE)
        .expect_err("定点加法溢出必须返回错误");
    assert_eq!(overflow, decimalx::DecimalError::RepresentationOverflow);
    let invalid_wire = format!(r#"{{"mantissa":"1","scale":{}}}"#, decimalx::MAX_SCALE + 1);
    assert!(serde_json::from_str::<decimalx::Decimal>(&invalid_wire).is_err());
}
