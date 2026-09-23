#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! Crate-外消费者路径：通过 lib 入口加载已交付类型并断言 checked 运算返回值。
//!
//! 非 mock、非 re-implementation：直接调用 `decimalx` 导出 API。

use decimalx::{
    Currency, Decimal, DecimalLimits, MAX_SCALE, Money, Price, Qty, Ratio, RoundingStrategy,
    TECH_MAX_POW10_EXP,
};

#[test]
fn entry_checked_add_one_plus_half() {
    // 1.00 + 0.50 = 1.50
    let a = Decimal::try_new(100, 2).expect("try_new 1.00");
    let b = Decimal::try_new(50, 2).expect("try_new 0.50");
    let sum = a.checked_add(b).expect("checked_add");
    assert_eq!(sum.mantissa(), 150);
    assert_eq!(sum.scale(), 2);
    assert_eq!(sum.to_string(), "1.5");
    assert!(sum.eq_value(Decimal::try_new(15, 1).unwrap()));
}

#[test]
fn entry_checked_sub_and_mul_concrete() {
    // 2.50 - 1.25 = 1.25
    let a = Decimal::try_new(250, 2).unwrap();
    let b = Decimal::try_new(125, 2).unwrap();
    let diff = a.checked_sub(b).expect("checked_sub");
    assert_eq!(diff.mantissa(), 125);
    assert_eq!(diff.scale(), 2);
    assert_eq!(diff.to_string(), "1.25");

    // 1.5 * 2 = 3.0
    let p = Decimal::try_new(15, 1)
        .unwrap()
        .checked_mul(Decimal::try_new(2, 0).unwrap())
        .expect("checked_mul");
    assert_eq!(p.mantissa(), 30);
    assert_eq!(p.scale(), 1);
    assert_eq!(p.to_string(), "3");
    assert!(p.eq_value(Decimal::try_new(3, 0).unwrap()));
}

#[test]
fn entry_checked_div_with_explicit_rounding() {
    // 1.00 / 0.50 = 2.00（HalfUp）
    let a = Decimal::try_new(100, 2).unwrap();
    let b = Decimal::try_new(50, 2).unwrap();
    let q = a.checked_div(b, RoundingStrategy::HalfUp).expect("checked_div");
    assert_eq!(q.mantissa(), 200);
    assert_eq!(q.scale(), 2);
    assert_eq!(q.to_string(), "2");
}

#[test]
fn entry_checked_rescale_concrete() {
    // 1.25 → scale 1 HalfUp → 1.3
    let d = Decimal::try_new(125, 2).unwrap();
    let r = d.checked_rescale(1, RoundingStrategy::HalfUp).expect("checked_rescale");
    assert_eq!(r.mantissa(), 13);
    assert_eq!(r.scale(), 1);
    assert_eq!(r.to_string(), "1.3");
}

#[test]
fn entry_div_zero_and_overflow_err() {
    let a = Decimal::try_new(1, 0).unwrap();
    let err = a.checked_div(Decimal::ZERO, RoundingStrategy::Floor).unwrap_err();
    assert!(err.to_string().contains("除零"));

    let overflow = Decimal::new(i128::MAX, 0).checked_add(Decimal::new(1, 0)).unwrap_err();
    assert!(matches!(
        overflow.kind(),
        decimalx::DecimalErrorKind::Representation | decimalx::DecimalErrorKind::Mantissa
    ));

    // i128 除法溢出：MIN / -1 必须 Err，不得 panic
    let div_overflow = Decimal::new(i128::MIN, 0)
        .checked_div(Decimal::new(-1, 0), RoundingStrategy::HalfUp)
        .expect_err("i128 MIN/-1 must return Err");
    assert!(matches!(
        div_overflow.kind(),
        decimalx::DecimalErrorKind::Mantissa | decimalx::DecimalErrorKind::Representation
    ));
}

#[test]
fn entry_money_currency_parse() {
    let c: Currency = "USD".parse().expect("currency parse");
    let m = Money::try_new(Decimal::try_new(999, 2).unwrap(), c).unwrap();
    assert_eq!(m.currency().as_str(), "USD");
    assert_eq!(m.amount().mantissa(), 999);
    // 生产路径接受 MAX_SCALE 并拒绝 +1（驱动真实 exported 常量）
    assert_eq!(MAX_SCALE, DecimalLimits::MAX_SCALE);
    assert_eq!(TECH_MAX_POW10_EXP, DecimalLimits::TECH_MAX_POW10_EXP);
    assert_eq!(MAX_SCALE, 18);
    assert!(Decimal::try_new(1, MAX_SCALE).is_ok());
    assert!(Decimal::try_new(1, MAX_SCALE.saturating_add(1)).is_err());
}

#[test]
fn entry_price_qty_ratio_newtypes() {
    let d = Decimal::try_new(1000, 2).unwrap(); // 10.00
    let price = Price::new(d);
    let qty = Qty::new(Decimal::try_new(3, 0).unwrap());
    let ratio = Ratio::new(Decimal::try_new(1, 2).unwrap()); // 0.01
    assert_eq!(price.as_decimal().to_string(), "10");
    assert_eq!(qty.as_decimal().mantissa(), 3);
    assert_eq!(ratio.as_decimal().scale(), 2);
    // 数值 Eq 经 newtype 字段穿透
    assert_eq!(price.as_decimal(), Decimal::try_new(1000, 2).unwrap());
    assert!(price.as_decimal().eq_value(Decimal::try_new(10, 0).unwrap()));
}
