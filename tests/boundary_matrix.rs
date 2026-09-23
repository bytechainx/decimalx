#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! 完整边界策略表驱动测试（W1 / DEFER-4）。

use decimalx::{
    Currency, Decimal, DecimalError, DecimalErrorKind, MAX_SCALE, Money, RoundingStrategy,
    TECH_MAX_POW10_EXP,
};
use proptest::prelude::*;
use std::error::Error;

#[test]
fn scale_bounds_try_new() {
    assert!(Decimal::try_new(1, 0).is_ok());
    assert!(Decimal::try_new(1, MAX_SCALE).is_ok());
    let err = Decimal::try_new(1, MAX_SCALE.saturating_add(1)).unwrap_err();
    assert_eq!(err.kind(), DecimalErrorKind::Scale);
}

#[test]
fn mantissa_extremes_construct() {
    assert!(Decimal::try_new(0, 0).is_ok());
    assert!(Decimal::try_new(i128::MAX, 0).is_ok());
    assert!(Decimal::try_new(i128::MIN, 0).is_ok());
    assert!(Decimal::try_new(i128::MAX, MAX_SCALE).is_ok());
    assert!(Decimal::try_new(i128::MIN, MAX_SCALE).is_ok());
}

#[test]
fn display_parse_roundtrip_covers_i128_minimum() {
    for scale in [0, 1, MAX_SCALE] {
        let value = Decimal::try_new(i128::MIN, scale).expect("合法 i128::MIN 十进制");
        let text = value.to_string();
        let reparsed: Decimal = text.parse().expect("Display 输出必须可被 FromStr 精确读回");
        assert_eq!(reparsed, value, "scale={scale}, text={text}");
    }
}

#[test]
fn oversized_fraction_length_never_wraps_in_diagnostic() {
    for (length, expected_kind) in [
        (19_usize, DecimalErrorKind::Scale),
        (255, DecimalErrorKind::Scale),
        (256, DecimalErrorKind::Parse),
    ] {
        let text = format!("0.{}", "1".repeat(length));
        let error = text.parse::<Decimal>().expect_err("超长小数位必须拒绝");
        assert_eq!(error.kind(), expected_kind, "小数位数 {length} 分类错误");
        assert!(error.to_string().contains(&length.to_string()), "诊断必须保留真实长度: {error}");
        assert!(!error.to_string().contains("scale 0"), "诊断不得发生窄化回绕: {error}");
    }
}

proptest! {
    /// 任意可表示 `i128 × scale` 都必须通过公开文本面精确往返。
    #[test]
    fn display_parse_roundtrip_property(mantissa in any::<i128>(), scale in 0_u8..=MAX_SCALE) {
        let value = Decimal::try_new(mantissa, scale).expect("生成器只产生合法 scale");
        let reparsed: Decimal = value.to_string().parse().expect("Display 输出必须可读回");
        prop_assert_eq!(reparsed, value);
    }
}

#[test]
fn decimal_error_conversion_preserves_source_chain() {
    let error: kernel::XError = DecimalError::DivisionByZero.into();
    let source = error.source().expect("DecimalError 必须保留为 XError source");
    assert!(
        source.downcast_ref::<DecimalError>().is_some(),
        "source 必须保留 DecimalError 类型身份"
    );
    assert_eq!(source.to_string(), "十进制除零");
}

#[test]
fn add_overflow_i128_max() {
    let err = Decimal::new(i128::MAX, 0).checked_add(Decimal::new(1, 0)).unwrap_err();
    assert!(matches!(err.kind(), DecimalErrorKind::Representation | DecimalErrorKind::Mantissa));
}

#[test]
fn sub_overflow_i128_min() {
    let err = Decimal::new(i128::MIN, 0).checked_sub(Decimal::new(1, 0)).unwrap_err();
    assert!(matches!(err.kind(), DecimalErrorKind::Representation | DecimalErrorKind::Mantissa));
}

#[test]
fn mul_overflow_large_mantissas() {
    let a = Decimal::new(i128::MAX / 2, 0);
    let b = Decimal::new(3, 0);
    let err = a.checked_mul(b).unwrap_err();
    assert_eq!(err.kind(), DecimalErrorKind::Mantissa);
}

#[test]
fn mul_scale_sum_exceeds_max() {
    // scale 10 + 10 = 20 > MAX_SCALE(18) → 结果非法
    let a = Decimal::new(2, 10);
    let b = Decimal::new(3, 10);
    let err = a.checked_mul(b).unwrap_err();
    assert!(matches!(
        err.kind(),
        DecimalErrorKind::Scale | DecimalErrorKind::Mantissa | DecimalErrorKind::Representation
    ));
}

#[test]
fn div_by_zero_and_min_neg_one() {
    let z = Decimal::new(1, 0).checked_div(Decimal::ZERO, RoundingStrategy::HalfUp).unwrap_err();
    assert_eq!(z.kind(), DecimalErrorKind::DivisionByZero);

    let o = Decimal::new(i128::MIN, 0)
        .checked_div(Decimal::new(-1, 0), RoundingStrategy::HalfUp)
        .unwrap_err();
    assert!(matches!(o.kind(), DecimalErrorKind::Mantissa | DecimalErrorKind::Representation));
}

#[test]
fn rescale_bounds() {
    let d = Decimal::new(125, 2);
    assert!(d.checked_rescale(MAX_SCALE, RoundingStrategy::HalfUp).is_ok());
    let err = d.checked_rescale(MAX_SCALE.saturating_add(1), RoundingStrategy::HalfUp).unwrap_err();
    assert_eq!(err.kind(), DecimalErrorKind::Scale);
}

#[test]
fn pow10_tech_limit_constant() {
    // 文档常量：i128 可表示 10^38
    assert_eq!(TECH_MAX_POW10_EXP, 38);
    // align 到 MAX_SCALE 在小 mantissa 上成功
    let d = Decimal::new(1, 0);
    assert!(d.checked_rescale(MAX_SCALE, RoundingStrategy::Floor).is_ok());
}

#[test]
fn serde_rejects_illegal_scale_and_currency() {
    let bad_scale = r#"{"mantissa":1,"scale":255}"#;
    assert!(serde_json::from_str::<Decimal>(bad_scale).is_err());

    // Currency wire = 3 字节数组（当前事实，见 docs/WIRE.md）
    let bad_ccy = r#"{"amount":{"mantissa":1,"scale":0},"currency":[117,115,100]}"#;
    assert!(serde_json::from_str::<Money>(bad_ccy).is_err());

    let ok = r#"{"mantissa":100,"scale":2}"#;
    let d: Decimal = serde_json::from_str(ok).unwrap();
    assert_eq!(d.mantissa(), 100);
    assert_eq!(d.scale(), 2);
}

#[test]
fn currency_and_money_validate() {
    assert!(Currency::try_new(*b"USD").is_ok());
    assert!(Currency::try_new(*b"usd").is_err());
    assert!("USDT".parse::<Currency>().is_err());
    let m = Money::try_new(Decimal::new(1, 0), Currency::try_new(*b"USD").unwrap()).unwrap();
    assert_eq!(m.currency().as_str(), "USD");
}

/// 表驱动：checked 路径对非法/极值不得 panic。
#[test]
fn checked_paths_no_panic_matrix() {
    let samples = [
        Decimal::new(0, 0),
        Decimal::new(1, MAX_SCALE),
        Decimal::new(-1, MAX_SCALE),
        Decimal::new(i128::MAX, 0),
        Decimal::new(i128::MIN, 0),
        Decimal::new(i128::MAX, MAX_SCALE),
        Decimal::new(i128::MIN, MAX_SCALE),
    ];
    for a in samples {
        for b in samples {
            let _ = a.checked_add(b);
            let _ = a.checked_sub(b);
            let _ = a.checked_mul(b);
            let _ = a.checked_div(b, RoundingStrategy::Floor);
            let _ = a.checked_div(b, RoundingStrategy::HalfUp);
            let _ = a.checked_rescale(0, RoundingStrategy::HalfEven);
            let _ = a.checked_rescale(MAX_SCALE, RoundingStrategy::Ceiling);
            let _ = format!("{a}"); // Display 对合法状态安全
        }
    }
}
