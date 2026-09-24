#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! decimalx 公开消费面：构造器、四则、newtypes、Money/Currency、错误与舍入。

use decimalx::{
    Currency, Decimal, DecimalError, DecimalErrorKind, DecimalLimits, DecimalResult, MAX_SCALE,
    Money, Price, Qty, Ratio, RoundingStrategy, TECH_MAX_POW10_EXP, WIRE_SCHEMA_VERSION,
};
use std::cmp::Ordering;
use std::str::FromStr;

#[test]
fn constants_and_limits() {
    assert_eq!(MAX_SCALE, 18);
    assert_eq!(TECH_MAX_POW10_EXP, 38);
    assert_eq!(DecimalLimits::MAX_SCALE, MAX_SCALE);
    assert_eq!(DecimalLimits::TECH_MAX_POW10_EXP, TECH_MAX_POW10_EXP);
    assert_eq!(WIRE_SCHEMA_VERSION, 1);
}

#[test]
fn decimal_construct_ops_normalize() {
    let z = Decimal::ZERO;
    assert_eq!(z.mantissa(), 0);
    let d = Decimal::new(1234, 2);
    assert_eq!(d.scale(), 2);
    assert!(d.is_within_limits());
    assert_eq!(d.validate().unwrap().mantissa(), 1234);
    assert!(Decimal::try_new(1, 99).is_err());

    let a = Decimal::new(10, 0);
    let b = Decimal::new(3, 0);
    assert_eq!(a.checked_add(b).unwrap().mantissa(), 13);
    assert_eq!(a.checked_sub(b).unwrap().mantissa(), 7);
    assert_eq!(a.checked_mul(b).unwrap().mantissa(), 30);
    let q = a.checked_div(b, RoundingStrategy::HalfEven).unwrap();
    assert_eq!(a.div(b, RoundingStrategy::HalfEven).unwrap(), q);
    assert_eq!(a.cmp_value(b), Ordering::Greater);
    assert!(a.eq_value(Decimal::new(100, 1)));
    let r = a.rescale(2, RoundingStrategy::HalfUp);
    assert_eq!(r.scale(), 2);
    assert!(a.checked_rescale(2, RoundingStrategy::HalfEven).is_ok());
    assert_eq!(Decimal::new(100, 2).normalize().mantissa(), 1);

    // FromStr / Display
    let parsed = Decimal::from_str("12.34").unwrap();
    assert_eq!(parsed.mantissa(), 1234);
    assert_eq!(parsed.scale(), 2);
    assert_eq!(parsed.to_string(), "12.34");
    assert!(Decimal::from_str("not-a-number").is_err());

    // 默认 feature 下生产路径走 checked_*（+/-/* 需 panicking-ops）
    assert_eq!(a.checked_add(b).unwrap().mantissa(), 13);
    assert_eq!(a.checked_sub(b).unwrap().mantissa(), 7);
    assert_eq!(a.checked_mul(b).unwrap().mantissa(), 30);

    // 除零
    let div0 = a.checked_div(Decimal::ZERO, RoundingStrategy::Floor).unwrap_err();
    assert_eq!(div0.kind(), DecimalErrorKind::DivisionByZero);
    assert!(div0.to_string().contains("除零"));
}

#[cfg(feature = "panicking-ops")]
#[test]
fn panicking_ops_available_when_feature_enabled() {
    let a = Decimal::new(10, 0);
    let b = Decimal::new(3, 0);
    assert_eq!((a + b).mantissa(), 13);
    assert_eq!((a - b).mantissa(), 7);
    assert_eq!((a * b).mantissa(), 30);
}

#[test]
fn all_rounding_strategies() {
    let a = Decimal::new(10, 0);
    let b = Decimal::new(3, 0);
    for strategy in [
        RoundingStrategy::Floor,
        RoundingStrategy::Ceiling,
        RoundingStrategy::HalfUp,
        RoundingStrategy::HalfDown,
        RoundingStrategy::HalfEven,
    ] {
        let q = a.checked_div(b, strategy).unwrap();
        assert!(q.mantissa() != 0 || a.mantissa() == 0);
        assert_eq!(a.div(b, strategy).unwrap(), q);
    }
}

#[test]
fn newtypes_money_currency_errors() {
    let d = Decimal::new(5, 0);
    assert_eq!(Price::new(d).as_decimal(), d);
    assert_eq!(Price::new(d).into_inner(), d);
    assert_eq!(Qty::new(d).as_decimal(), d);
    assert_eq!(Qty::new(d).into_inner(), d);
    assert_eq!(Ratio::new(d).as_decimal(), d);
    assert_eq!(Ratio::new(d).into_inner(), d);

    let c = Currency::try_new(*b"USD").unwrap();
    assert_eq!(c.as_str(), "USD");
    assert_eq!(c.as_bytes(), *b"USD");
    assert!(c.is_valid());
    assert!(c.validate().is_ok());
    assert!(Currency::try_new(*b"usd").is_err());
    let c2 = Currency::from_str("EUR").unwrap();
    assert_eq!(c2.as_str(), "EUR");
    assert!(Currency::from_str("us").is_err());
    assert!(Currency::from_str("usd").is_err());

    let m = Money::try_new(d, c).unwrap();
    assert_eq!(m.amount(), d);
    assert_eq!(m.currency(), c);
    assert!(m.validate().is_ok());

    let err = Decimal::try_new(1, 99).unwrap_err();
    assert_eq!(err.kind(), DecimalErrorKind::Scale);
    assert!(err.to_string().contains("scale"));
    let _k: DecimalErrorKind = err.kind();
    let _r: DecimalResult<Decimal> = Err(err);

    // 全部错误变体 kind + Display
    for (e, kind, needle) in [
        (
            DecimalError::ScaleOutOfRange { scale: 99, max: MAX_SCALE },
            DecimalErrorKind::Scale,
            "scale",
        ),
        (DecimalError::MantissaOverflow, DecimalErrorKind::Mantissa, "mantissa"),
        (DecimalError::DivisionByZero, DecimalErrorKind::DivisionByZero, "除零"),
        (DecimalError::RoundingOverflow, DecimalErrorKind::Rounding, "舍入"),
        (DecimalError::RepresentationOverflow, DecimalErrorKind::Representation, "表示"),
        (DecimalError::Parse("x".into()), DecimalErrorKind::Parse, "解析"),
        (DecimalError::InvalidCurrency, DecimalErrorKind::Currency, "币种"),
    ] {
        assert_eq!(e.kind(), kind);
        assert!(e.to_string().contains(needle), "display {:?} missing {needle}", e.to_string());
    }

    // XError 转换
    let xe: kernel::XError = DecimalError::InvalidCurrency.into();
    assert_eq!(xe.kind(), kernel::ErrorKind::Invalid);

    // serde 往返（当前字段 shape 事实）
    let json = serde_json::to_string(&d).unwrap();
    let back: Decimal = serde_json::from_str(&json).unwrap();
    assert_eq!(back, d);
    let m_json = serde_json::to_string(&m).unwrap();
    let m_back: Money = serde_json::from_str(&m_json).unwrap();
    assert_eq!(m_back.amount(), d);
    assert_eq!(m_back.currency().as_str(), "USD");
}
