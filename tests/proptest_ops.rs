#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! decimalx 属性测试（proptest）—— 交换/结合/单位元与 checked 无 panic。
//!
//! 不宣称全 i128 状态空间覆盖（见 residual T-DEF-003）。

use decimalx::{Decimal, RoundingStrategy};
use proptest::prelude::*;

/// 生成合理范围的 Decimal（mantissa i64 范围, scale u8 ≤ 18）。
fn arb_decimal() -> impl Strategy<Value = Decimal> {
    (any::<i64>(), 0u8..=18).prop_map(|(m, s)| Decimal::new(m as i128, s))
}

proptest! {
    /// 加法交换律：a + b == b + a（值相等；溢出两侧同 Err）
    #[test]
    fn add_commutative(a in arb_decimal(), b in arb_decimal()) {
        match (a.checked_add(b), b.checked_add(a)) {
            (Ok(ab), Ok(ba)) => prop_assert!(ab.eq_value(ba)),
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "checked_add asymmetry"),
        }
    }

    /// 加法结合律（checked；任一侧溢出则跳过值断言）
    #[test]
    fn add_associative(a in arb_decimal(), b in arb_decimal(), c in arb_decimal()) {
        let left = a.checked_add(b).and_then(|ab| ab.checked_add(c));
        let right = b.checked_add(c).and_then(|bc| a.checked_add(bc));
        match (left, right) {
            (Ok(l), Ok(r)) => prop_assert!(l.eq_value(r)),
            (Err(_), Err(_)) => {}
            // 一侧溢出一侧成功在边界上可能出现（不同对齐路径），不强制对称
            _ => {}
        }
    }

    /// 加法单位元：a + 0 == a
    #[test]
    fn add_identity(a in arb_decimal()) {
        let result = a.checked_add(Decimal::ZERO).expect("add 0");
        prop_assert!(result.eq_value(a));
    }

    /// 减法逆元：a - a == 0
    #[test]
    fn sub_self_is_zero(a in arb_decimal()) {
        let result = a.checked_sub(a).expect("sub self");
        prop_assert!(result.eq_value(Decimal::ZERO));
    }

    /// 乘法交换律
    #[test]
    fn mul_commutative(a in arb_decimal(), b in arb_decimal()) {
        let ab = a.checked_mul(b);
        let ba = b.checked_mul(a);
        match (ab, ba) {
            (Ok(x), Ok(y)) => prop_assert!(x.eq_value(y)),
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "checked_mul asymmetry"),
        }
    }

    /// 乘法单位元：a * 1 == a
    #[test]
    fn mul_identity(a in arb_decimal()) {
        let one = Decimal::new(1, 0);
        let result = a.checked_mul(one).expect("mul by 1");
        prop_assert!(result.eq_value(a));
    }

    /// 乘法零元：a * 0 == 0
    #[test]
    fn mul_zero(a in arb_decimal()) {
        let result = a.checked_mul(Decimal::ZERO).expect("mul by 0");
        prop_assert!(result.eq_value(Decimal::ZERO));
    }

    /// 除法不 panic（除数非零时）
    #[test]
    fn div_no_panic(a in arb_decimal(), b in arb_decimal()) {
        if b.mantissa() != 0 {
            let _ = a.div(b, RoundingStrategy::HalfUp);
            let _ = a.div(b, RoundingStrategy::Floor);
            let _ = a.div(b, RoundingStrategy::Ceiling);
            let _ = a.div(b, RoundingStrategy::HalfDown);
            let _ = a.div(b, RoundingStrategy::HalfEven);
        }
    }

    /// 除法除以零返回 Err
    #[test]
    fn div_by_zero_errors(a in arb_decimal()) {
        let result = a.div(Decimal::ZERO, RoundingStrategy::HalfUp);
        prop_assert!(result.is_err());
    }

    /// 任意运算：checked 路径不 panic
    #[test]
    fn no_panic_arbitrary_ops(a in arb_decimal(), b in arb_decimal()) {
        let _ = a.checked_add(b);
        let _ = a.checked_sub(b);
        let _ = a.checked_mul(b);
        if b.mantissa() != 0 {
            let _ = a.div(b, RoundingStrategy::HalfUp);
        }
    }

    /// 值相等具有自反/对称性；与 cmp 一致
    #[test]
    fn value_eq_reflexive_symmetric(a in arb_decimal(), b in arb_decimal()) {
        prop_assert!(a.eq_value(a));
        prop_assert_eq!(a.eq_value(b), b.eq_value(a));
        prop_assert_eq!(a.eq_value(b), a == b);
        prop_assert_eq!(a.cmp_value(b) == std::cmp::Ordering::Equal, a == b);
    }
}
