//! `Decimal` 的字符串解析与规范化展示实现。

use super::*;
use std::fmt;
use std::str::FromStr;

impl FromStr for Decimal {
    type Err = DecimalError;

    /// 解析十进制字符串（如 `"100"` / `"100.5"` / `"-1.25"`）。
    ///
    /// 禁止 `NaN` / `Inf` 等非有限表示；非法输入返回 [`DecimalError`]。
    fn from_str(s: &str) -> DecimalResult<Self> {
        let s = s.trim();
        if s.is_empty() {
            return Err(DecimalError::Parse("空字符串".into()));
        }

        let lower = s.to_ascii_lowercase();
        if matches!(
            lower.as_str(),
            "nan" | "inf" | "+inf" | "-inf" | "infinity" | "+infinity" | "-infinity"
        ) {
            return Err(DecimalError::Parse("不允许 NaN/Inf".into()));
        }

        let (negative, body) = if let Some(rest) = s.strip_prefix('-') {
            (true, rest)
        } else if let Some(rest) = s.strip_prefix('+') {
            (false, rest)
        } else {
            (false, s)
        };

        if body.is_empty() {
            return Err(DecimalError::Parse(format!("非法十进制: {s}")));
        }

        if body.bytes().filter(|&b| b == b'.').count() > 1 {
            return Err(DecimalError::Parse(format!("非法十进制: {s}")));
        }

        let (int_part, frac_part) = match body.split_once('.') {
            Some((i, f)) => (i, f),
            None => (body, ""),
        };

        if int_part.is_empty() && frac_part.is_empty() {
            return Err(DecimalError::Parse(format!("非法十进制: {s}")));
        }
        if !int_part.is_empty() && !int_part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(DecimalError::Parse(format!("非法十进制: {s}")));
        }
        if !frac_part.bytes().all(|b| b.is_ascii_digit()) {
            return Err(DecimalError::Parse(format!("非法十进制: {s}")));
        }

        let fraction_len = frac_part.len();
        let scale = u8::try_from(fraction_len).map_err(|_| {
            DecimalError::Parse(format!("小数位数 {fraction_len} 超过可表示范围 {}", u8::MAX))
        })?;
        if scale > MAX_SCALE {
            return Err(DecimalError::ScaleOutOfRange { scale, max: MAX_SCALE });
        }

        let digits = if int_part.is_empty() {
            frac_part.to_string()
        } else if frac_part.is_empty() {
            int_part.to_string()
        } else {
            format!("{int_part}{frac_part}")
        };

        let magnitude: u128 = if digits.is_empty() || digits.bytes().all(|b| b == b'0') {
            0
        } else {
            digits.parse::<u128>().map_err(|_| DecimalError::MantissaOverflow)?
        };

        let mantissa = if negative {
            const I128_MIN_MAGNITUDE: u128 = i128::MIN.unsigned_abs();
            if magnitude == I128_MIN_MAGNITUDE {
                i128::MIN
            } else {
                let positive: i128 =
                    magnitude.try_into().map_err(|_| DecimalError::MantissaOverflow)?;
                -positive
            }
        } else {
            magnitude.try_into().map_err(|_| DecimalError::MantissaOverflow)?
        };

        Decimal { mantissa, scale }.finish()
    }
}

impl fmt::Display for Decimal {
    /// 规范化展示：去掉无意义尾随小数零；纯整数不带小数点。
    ///
    /// 例：`100.0` → `"100"`，`10.50` → `"10.5"`，`0.00` → `"0"`。
    #[allow(clippy::expect_used)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let n = self.normalize();
        if n.scale == 0 {
            return write!(f, "{}", n.mantissa);
        }

        let neg = n.mantissa < 0;
        let abs = n.mantissa.unsigned_abs();
        // 不变量：scale ≤ MAX_SCALE 时 10^scale 可装入 u128
        let divisor = 10u128
            .checked_pow(u32::from(n.scale))
            // PANIC: n.scale 受 MAX_SCALE 限制，10 的该次幂可由 u128 表示。
            .expect("scale <= MAX_SCALE ensures 10^scale fits u128");
        let int_part = abs / divisor;
        let frac_part = abs % divisor;
        if neg {
            write!(f, "-{int_part}.{:0width$}", frac_part, width = n.scale as usize)
        } else {
            write!(f, "{int_part}.{:0width$}", frac_part, width = n.scale as usize)
        }
    }
}
