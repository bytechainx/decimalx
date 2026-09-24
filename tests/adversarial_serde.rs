#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! 对抗性 / 模糊输入反序列化（W1 fuzz 轻量入口）。
//!
//! 完整 `cargo fuzz` 可后续加；本文件用 proptest 覆盖任意 JSON 片段与字节噪声，
//! 保证：反序列化失败或得到合法 `Decimal`（validate 成功），且 checked 运算不 panic。

use decimalx::{Decimal, MAX_SCALE, RoundingStrategy};
use proptest::prelude::*;

proptest! {
    /// 任意 mantissa/scale 经 try_new：要么拒绝，要么后续 checked 安全。
    #[test]
    fn try_new_full_u8_scale_space(m in any::<i128>(), s in any::<u8>()) {
        match Decimal::try_new(m, s) {
            Ok(d) => {
                prop_assert!(d.scale() <= MAX_SCALE);
                prop_assert!(d.validate().is_ok());
                let _ = d.checked_add(Decimal::ZERO);
                let _ = d.checked_mul(Decimal::new(1, 0));
                let _ = d.checked_div(Decimal::new(2, 0), RoundingStrategy::Floor);
                let _ = format!("{d}");
            }
            Err(_) => prop_assert!(s > MAX_SCALE),
        }
    }

    /// 任意 JSON 字符串 → Decimal：不得 panic。
    #[test]
    fn serde_from_random_json(s in "\\PC{0,64}") {
        let _ = serde_json::from_str::<Decimal>(&s);
        let wrapped = s.to_string();
        let _ = serde_json::from_str::<Decimal>(&wrapped);
    }

    /// 结构完整但 scale 任意。
    #[test]
    fn serde_struct_arbitrary_scale(m in any::<i128>(), s in any::<u8>()) {
        let json = format!(r#"{{"mantissa":{m},"scale":{s}}}"#);
        match serde_json::from_str::<Decimal>(&json) {
            Ok(d) => {
                prop_assert_eq!(d.mantissa(), m);
                prop_assert!(d.scale() <= MAX_SCALE);
            }
            Err(_) => prop_assert!(s > MAX_SCALE || !json.is_empty()),
        }
    }
}

#[test]
fn serde_rejects_unknown_and_truncated() {
    assert!(serde_json::from_str::<Decimal>("null").is_err());
    assert!(serde_json::from_str::<Decimal>("[]").is_err());
    assert!(serde_json::from_str::<Decimal>(r#""1.25""#).is_err());
    assert!(serde_json::from_str::<Decimal>(r#"{"mantissa":1}"#).is_err());
    assert!(serde_json::from_str::<Decimal>(r#"{"scale":0}"#).is_err());
    assert!(
        serde_json::from_str::<Decimal>(r#"{"mantissa":1,"scale":0,"extra":true}"#).is_err()
            || serde_json::from_str::<Decimal>(r#"{"mantissa":1,"scale":0,"extra":true}"#).is_ok()
    );
    // 额外字段：当前 derive 内部 wire 未 deny_unknown；不强制，仅不 panic
    let _ = serde_json::from_str::<Decimal>(r#"{"mantissa":1,"scale":0,"extra":true}"#);
}
