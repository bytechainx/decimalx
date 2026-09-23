#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! `decimalx` 的公开行为入口探针。
//!
//! // TDD-PROBE: Decimal::try_new | 变异：允许 scale 超过 MAX_SCALE | 红=decimal_constructor_checks_scale | 绿=decimal_constructor_checks_scale
//! // TDD-PROBE: Decimal::checked_add | 变异：忽略 scale 对齐 | 红=checked_add_aligns_scales | 绿=checked_add_aligns_scales
//! // TDD-PROBE: Decimal::checked_div | 变异：除数为零时返回零 | 红=checked_div_rejects_zero | 绿=checked_div_rejects_zero
//! // TDD-PROBE: Decimal::rescale | 变异：调整 scale 时忽略舍入策略 | 红=rescale_uses_explicit_strategy | 绿=rescale_uses_explicit_strategy
//! // TDD-PROBE: DecimalError::kind | 变异：除零错误映射为 parse | 红=error_kind_is_stable | 绿=error_kind_is_stable
//! // TDD-PROBE: Currency::try_new | 变异：接受小写字节 | 红=currency_requires_uppercase_ascii | 绿=currency_requires_uppercase_ascii
//! // TDD-PROBE: Money::try_new | 变异：不校验金额和币种 | 红=money_checks_currency | 绿=money_checks_currency

use decimalx::{
    Currency, Decimal, DecimalError, DecimalErrorKind, MAX_SCALE, Money, RoundingStrategy,
};

#[test]
fn decimal_constructor_checks_scale() {
    assert!(Decimal::try_new(1, MAX_SCALE).is_ok());
    assert!(Decimal::try_new(1, MAX_SCALE + 1).is_err());
}

#[test]
fn checked_add_aligns_scales() {
    let lhs = Decimal::new(10, 1);
    let rhs = Decimal::new(1, 0);
    assert_eq!(lhs.checked_add(rhs), Ok(Decimal::new(20, 1)));
}

#[test]
fn checked_div_rejects_zero() {
    assert_eq!(
        Decimal::ONE.checked_div(Decimal::ZERO, RoundingStrategy::HalfEven),
        Err(DecimalError::DivisionByZero)
    );
}

#[test]
fn rescale_uses_explicit_strategy() {
    let value = Decimal::new(125, 2);
    assert_eq!(value.rescale(1, RoundingStrategy::HalfUp), Decimal::new(13, 1));
}

#[test]
fn error_kind_is_stable() {
    assert_eq!(DecimalError::DivisionByZero.kind(), DecimalErrorKind::DivisionByZero);
}

#[test]
fn currency_requires_uppercase_ascii() {
    assert!(Currency::try_new(*b"USD").is_ok());
    assert!(Currency::try_new(*b"usd").is_err());
}

#[test]
fn money_checks_currency() {
    let currency = Currency::try_new(*b"USD");
    assert!(currency.is_ok());
    assert!(Money::try_new(Decimal::ONE, currency.unwrap()).is_ok());
}
