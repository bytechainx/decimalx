#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! `decimalx` 的 AI 生成边界候选；逐条人工复核后保留。
//!
//! // AIDD: scale 超过公开上限 | 来源=AI | 复核=ZoneCNH/2026-09-24 | 依据=标准.md §约束，拒绝非法 scale | 结论=保留
//! // AIDD: i128 最大值继续加一 | 来源=AI | 复核=ZoneCNH/2026-09-24 | 依据=标准.md §约束，溢出以 Err 表示 | 结论=保留
//! // AIDD: 任意值除以零 | 来源=AI | 复核=ZoneCNH/2026-09-24 | 依据=标准.md §范围，除零显式返回错误 | 结论=保留
//! // AIDD: 币种含小写字母 | 来源=AI | 复核=ZoneCNH/2026-09-24 | 依据=标准.md §约束，币种为大写 ASCII | 结论=保留
//! // AIDD: 货币金额加法溢出 | 来源=AI | 复核=ZoneCNH/2026-09-24 | 依据=标准.md §约束，checked 路径不得 panic | 结论=保留

use decimalx::{Currency, Decimal, DecimalError, MAX_SCALE};

#[test]
fn scale_above_limit_is_rejected() {
    assert!(matches!(
        Decimal::try_new(1, MAX_SCALE + 1),
        Err(DecimalError::ScaleOutOfRange { .. })
    ));
}

#[test]
fn maximum_mantissa_addition_reports_overflow() {
    assert_eq!(Decimal::MAX.checked_add(Decimal::ONE), Err(DecimalError::RepresentationOverflow));
}

#[test]
fn division_by_zero_is_an_error() {
    assert_eq!(
        Decimal::ONE.checked_div(Decimal::ZERO, decimalx::RoundingStrategy::HalfUp),
        Err(DecimalError::DivisionByZero)
    );
}

#[test]
fn lowercase_currency_is_rejected() {
    assert!(Currency::try_new(*b"usd").is_err());
}

#[test]
fn overflow_remains_fallible_in_checked_path() {
    assert!(Decimal::MAX.checked_mul(Decimal::TEN).is_err());
}
