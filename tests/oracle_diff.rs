#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! 独立高精度 oracle 差分（W1 / DEFER-4）。
//!
//! # Oracle 边界（正式合同）
//! - 使用 `bigdecimal` **仅 dev-dep** 作为参考实现。
//! - **仅**在 `decimalx` 返回 `Ok` 时与 oracle 比数值；`Err` 表示本 crate
//!   `i128` 中间值能力限制（见 crate rustdoc），oracle 可能仍可算——**不**视为 bug。
//! - 除法：仅比对 `RoundingStrategy::Floor` 与向 −∞ 截断语义一致的样例子集；
//!   HalfUp 等银行家舍入不在本文件与 BigDecimal 默认行为强制对齐。
//! - 输入域：property 生成 `i64` mantissa + `0..=18` scale（与现有 proptest 一致）；
//!   极值单测覆盖 `i128` 边界。

use bigdecimal::BigDecimal;
use decimalx::{Decimal, RoundingStrategy};
use proptest::prelude::*;
use std::str::FromStr;

fn to_big(d: Decimal) -> BigDecimal {
    // 经十进制文本进入 oracle，避免直接依赖 num-bigint 类型路径差异
    BigDecimal::from_str(&d.to_string())
        .unwrap_or_else(|e| panic!("oracle parse decimalx Display {d}: {e}"))
}

fn arb_decimal() -> impl Strategy<Value = Decimal> {
    (any::<i64>(), 0u8..=18).prop_map(|(m, s)| Decimal::new(i128::from(m), s))
}

/// 当两侧都成功时，数值应与 BigDecimal 一致（允许 scale 规范化差异）。
fn assert_same_value(dx: Decimal, bd: BigDecimal, ctx: &str) {
    let left = to_big(dx).normalized();
    let right = bd.normalized();
    assert_eq!(left, right, "{ctx}: decimalx={dx} ({left}) oracle={right}");
}

proptest! {
    #[test]
    fn oracle_checked_add(a in arb_decimal(), b in arb_decimal()) {
        if let Ok(sum) = a.checked_add(b) {
            let expect = to_big(a) + to_big(b);
            assert_same_value(sum, expect, "checked_add");
        }
    }

    #[test]
    fn oracle_checked_sub(a in arb_decimal(), b in arb_decimal()) {
        if let Ok(diff) = a.checked_sub(b) {
            let expect = to_big(a) - to_big(b);
            assert_same_value(diff, expect, "checked_sub");
        }
    }

    #[test]
    fn oracle_checked_mul(a in arb_decimal(), b in arb_decimal()) {
        if let Ok(prod) = a.checked_mul(b) {
            let expect = to_big(a) * to_big(b);
            assert_same_value(prod, expect, "checked_mul");
        }
    }
}

#[test]
fn oracle_checked_div_floor_concrete() {
    // 10 / 3 → scale max=0 → Floor → 3
    let a = Decimal::new(10, 0);
    let b = Decimal::new(3, 0);
    let q = a.checked_div(b, RoundingStrategy::Floor).expect("div");
    assert_eq!(q.mantissa(), 3);
    assert_eq!(q.scale(), 0);

    // 1.00 / 0.30 → scale 2；Floor
    let a = Decimal::new(100, 2);
    let b = Decimal::new(30, 2);
    let q = a.checked_div(b, RoundingStrategy::Floor).expect("div");
    // 1.00/0.30 = 3.333... → Floor at scale 2 → 3.33
    assert_eq!(q.to_string(), "3.33");
}

#[test]
fn oracle_parse_roundtrip_samples() {
    for s in ["0", "1", "-1", "1.25", "0.000000000000000001", "999999999999999999"] {
        let d: Decimal = s.parse().expect("parse");
        let bd = BigDecimal::from_str(s).expect("big parse");
        // Display 可能去掉尾零；用 BigDecimal 比数值
        assert_eq!(to_big(d).normalized(), bd.normalized(), "sample {s}");
    }
}

#[test]
fn oracle_documents_intermediate_overflow_is_not_bug() {
    // 若两边对齐需要很大 pow10，decimalx 可能 RepresentationOverflow，
    // 而 BigDecimal 仍可加。合同：Ok 时一致；Err 可接受。
    let a = Decimal::new(i128::MAX / 10, 0);
    let b = Decimal::new(1, 18);
    match a.checked_add(b) {
        Ok(v) => {
            let expect = to_big(a) + to_big(b);
            assert_same_value(v, expect, "wide add ok path");
        }
        Err(e) => {
            assert!(
                matches!(
                    e.kind(),
                    decimalx::DecimalErrorKind::Representation
                        | decimalx::DecimalErrorKind::Mantissa
                ),
                "expected capability Err, got {e:?}"
            );
        }
    }
}
