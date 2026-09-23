//! decimalx —— `/types/` 十进制数值类型（ADR-006/007，spec §4.2）。
//!
//! 纯基础数值层，无业务逻辑。`Decimal` 族唯一定义点（ADR-007）。
//! 禁止 `f32`/`f64` 参与任何金额/数量运算；除法必须显式指定 [`RoundingStrategy`]。
//!
//! ## 契约要点（ADR-006）
//! - 双目加减/比较：scale 不一致时对齐到较大 scale（不足位补零，不隐式舍入）
//! - 除法：必须显式 `RoundingStrategy`；结果 scale = `max(lhs.scale, rhs.scale)`
//! - 缩位：[`Decimal::rescale`] / [`Decimal::checked_rescale`]
//! - 溢出：checked API 返回 `Err`；运算符在溢出时 panic（禁止静默回绕）
//!
//! ## 生产路径（P0）
//! - 构造：[`Decimal::try_new`] / [`str::parse`] / [`Decimal::new`]（均强制 `scale ≤ MAX_SCALE`）
//! - 字段私有：非法 scale 不可表示；serde 反序列化走校验路径
//! - 运算：`checked_*` 对可达状态只返回 `Ok/Err`，不 panic；**资金路径必须用 checked API**
//! - panicking 运算符 `+/-/*` 仅在 feature `panicking-ops` 下公开（**默认关闭**）
//! - 错误：[`DecimalError`] 区分 scale / mantissa / 除零 / 舍入 / 表示范围（中文 Display）
//! - 中间值合同：`i128` 中间值溢出则 `Err`，即使约分后可表示
//! - wire：serde 字段 shape 由 [`WIRE_SCHEMA_VERSION`] 标识；JSON 交换剖面 `mantissa` 为十进制字符串（读兼容历史 JSON 整数）；见 `docs/WIRE.md`

#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)
)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]
#![deny(unreachable_pub)]

use serde::de::{self, Deserializer};
use serde::{Deserialize, Serialize, Serializer};
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

mod error;
mod text;
mod types;

pub use error::{DecimalError, DecimalErrorKind};
pub use types::{Currency, Money, Price, Qty, Ratio};

/// 十进制 API 的 `Result` 别名。
pub type DecimalResult<T> = Result<T, DecimalError>;

/// 生产 fallible API 强制的最大 scale（常见 NUMERIC(38,18) / 交易所精度）。
pub const MAX_SCALE: u8 = 18;

/// i128 可表示的最大 `10^exp` 指数。
pub const TECH_MAX_POW10_EXP: u32 = 38;

/// 当前 wire schema 版本（`Decimal` / `Money` / `Currency` JSON 字段形状）。
///
/// - **v1**：`Decimal` = `{mantissa, scale}`；`Money` = `{amount, currency}`；
///   `Currency` = `[u8; 3]` 大写 ASCII；均 `deny_unknown_fields`（Decimal/Money）。
/// - 破坏性字段 rename / shape 变更必须递增本常量，并由 `wire_schema_v1_*` 测试拦截。
pub const WIRE_SCHEMA_VERSION: u32 = 1;

/// 十进制表示边界（生产 fallible 路径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecimalLimits;

impl DecimalLimits {
    /// 与 [`MAX_SCALE`] 相同。
    pub const MAX_SCALE: u8 = MAX_SCALE;
    /// 与 [`TECH_MAX_POW10_EXP`] 相同。
    pub const TECH_MAX_POW10_EXP: u32 = TECH_MAX_POW10_EXP;
}

/// 十进制数：`mantissa × 10^(-scale)`（spec §4.2）。
///
/// 相等性与排序按**数值**（scale 对齐后）比较，而非按字段结构。
/// serde 按 `{mantissa, scale}` 字段序列化；**写入**时 `mantissa` 为十进制字符串；
/// **读取**兼容历史 JSON 整数 `mantissa`；反序列化强制 `scale ≤ MAX_SCALE`。
/// 字段私有：非法 scale 不可表示。
///
/// # Examples
///
/// ```
/// let value = decimalx::Decimal::try_new(125, 2);
/// assert!(value.is_ok());
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Decimal {
    /// 定点整数 mantissa。
    mantissa: i128,
    /// 小数位数；恒满足 `≤ MAX_SCALE`。
    scale: u8,
}

impl Decimal {
    /// 构造合法十进制；`scale > MAX_SCALE` 时 panic（const 友好）。
    ///
    /// 生产 fallible 路径请用 [`Self::try_new`]。
    ///
    /// # Panics
    ///
    /// `scale > MAX_SCALE` 时 panic。
    pub const fn new(mantissa: i128, scale: u8) -> Self {
        assert!(scale <= MAX_SCALE, "十进制小数位数超过 MAX_SCALE");
        Self { mantissa, scale }
    }

    /// 生产推荐构造：拒绝 `scale > MAX_SCALE`。
    pub fn try_new(mantissa: i128, scale: u8) -> DecimalResult<Self> {
        if scale > MAX_SCALE {
            return Err(DecimalError::ScaleOutOfRange { scale, max: MAX_SCALE });
        }
        Ok(Self { mantissa, scale })
    }

    /// 定点 mantissa。
    pub const fn mantissa(self) -> i128 {
        self.mantissa
    }

    /// 小数位数。
    pub const fn scale(self) -> u8 {
        self.scale
    }

    /// 当前值是否满足生产 scale 上限（构造成功后恒为 true）。
    pub const fn is_within_limits(self) -> bool {
        self.scale <= MAX_SCALE
    }

    /// 校验 scale 上限；失败返回 `Err`（对已构造值恒 Ok）。
    pub fn validate(self) -> DecimalResult<Self> {
        Self::try_new(self.mantissa, self.scale)
    }

    /// 零值：`0 × 10^0`。
    pub const ZERO: Self = Self { mantissa: 0, scale: 0 };

    /// 一值：`1 × 10^0`。
    pub const ONE: Self = Self { mantissa: 1, scale: 0 };

    /// 二值：`2 × 10^0`。
    pub const TWO: Self = Self { mantissa: 2, scale: 0 };

    /// 十值：`10 × 10^0`（对齐 `rust_decimal::Decimal::TEN`）。
    pub const TEN: Self = Self { mantissa: 10, scale: 0 };

    /// 负一：`-1 × 10^0`（对齐 `rust_decimal::Decimal::NEGATIVE_ONE`）。
    pub const NEGATIVE_ONE: Self = Self { mantissa: -1, scale: 0 };

    /// 最大可表示整数：`i128::MAX × 10^0`（scale=0 下按 [`Ord`] 最大）。
    pub const MAX: Self = Self { mantissa: i128::MAX, scale: 0 };

    /// 最小可表示整数：`i128::MIN × 10^0`（scale=0 下按 [`Ord`] 最小）。
    pub const MIN: Self = Self { mantissa: i128::MIN, scale: 0 };

    /// 是否为零值（任意 scale 下的 0 均判定为零）。
    pub fn is_zero(self) -> bool {
        self.mantissa == 0
    }

    /// 是否严格为负（对齐 `rust_decimal::Decimal::is_sign_negative`；`-0` 视为 false）。
    pub fn is_sign_negative(self) -> bool {
        self.mantissa < 0
    }

    /// 是否严格为正（对齐 `rust_decimal::Decimal::is_sign_positive`；`0` 视为 false）。
    pub fn is_sign_positive(self) -> bool {
        self.mantissa > 0
    }

    /// 绝对值：`mantissa` 取绝对值，`scale` 不变。
    ///
    /// Fallible：`i128::MIN` 的绝对值（2^127）无法用 `i128` 表示，
    /// 此时返回 [`DecimalError::RepresentationOverflow`]（旧实现 `self.mantissa.abs()`
    /// 在 debug 构建下对 `i128::MIN` panic，属未定义行为；release 构建下静默回绕）。
    pub fn abs(self) -> DecimalResult<Self> {
        if self.mantissa == i128::MIN {
            return Err(DecimalError::RepresentationOverflow);
        }
        Ok(Self { mantissa: self.mantissa.unsigned_abs() as i128, scale: self.scale })
    }

    /// 精确解析十进制字符串（与 [`FromStr`] 同语义）。
    ///
    /// 命名对齐 `rust_decimal::Decimal::from_str_exact`，便于底座迁移；
    /// 生产路径亦可直接 `s.parse::<Decimal>()`。
    pub fn from_str_exact(s: &str) -> DecimalResult<Self> {
        s.parse()
    }

    /// 运算结果检查：若 scale 越界，先 `normalize` 去掉尾随零再判定（保留可精确表示的乘积）。
    fn finish(self) -> DecimalResult<Self> {
        if self.scale <= MAX_SCALE {
            return Ok(self);
        }
        let n = self.normalize();
        if n.scale <= MAX_SCALE {
            return Ok(n);
        }
        Err(DecimalError::ScaleOutOfRange { scale: self.scale, max: MAX_SCALE })
    }

    /// `10^exp`，溢出时返回 `None`。
    fn pow10(exp: u32) -> Option<i128> {
        10i128.checked_pow(exp)
    }

    /// 对齐到 `target` scale（`target >= self.scale` 时乘以 10 的幂）；禁止静默回绕。
    #[allow(clippy::expect_used)]
    fn try_align_scale(self, target: u8) -> DecimalResult<Decimal> {
        if target > MAX_SCALE {
            return Err(DecimalError::ScaleOutOfRange { scale: target, max: MAX_SCALE });
        }
        if self.scale >= target {
            return Ok(self);
        }
        let diff = u32::from(target - self.scale);
        // 不变量：target ≤ MAX_SCALE 且 self.scale ≥ 0 ⇒ diff ≤ MAX_SCALE，pow10 必落入 i128
        // PANIC: 上述范围约束保证该幂次可表示；约束变化时应改为传播错误。
        let factor = Self::pow10(diff).expect("diff <= MAX_SCALE 保证 10 的幂可由 i128 表示");
        let mantissa =
            self.mantissa.checked_mul(factor).ok_or(DecimalError::RepresentationOverflow)?;
        Ok(Decimal { mantissa, scale: target })
    }

    /// 去掉尾随零，得到值相等的规范表示（用于 Hash）。
    fn canonical(self) -> (i128, u8) {
        if self.mantissa == 0 {
            return (0, 0);
        }
        let mut m = self.mantissa;
        let mut s = self.scale;
        while s > 0 && m % 10 == 0 {
            m /= 10;
            s -= 1;
        }
        (m, s)
    }

    /// 数值比较（ADR-006：对齐到较大 scale）。对齐溢出时按量级判定。
    pub fn cmp_value(self, other: Decimal) -> Ordering {
        if self.mantissa == 0 && other.mantissa == 0 {
            return Ordering::Equal;
        }
        if self.scale == other.scale {
            return self.mantissa.cmp(&other.mantissa);
        }
        if self.scale < other.scale {
            let diff = u32::from(other.scale - self.scale);
            match Self::pow10(diff).and_then(|f| self.mantissa.checked_mul(f)) {
                Some(aligned) => aligned.cmp(&other.mantissa),
                None => {
                    // |self| 在 other 的 scale 下无法装入 i128 → |self| 更大
                    if self.mantissa > 0 { Ordering::Greater } else { Ordering::Less }
                }
            }
        } else {
            other.cmp_value(self).reverse()
        }
    }

    /// 比较：scale 对齐后值相同。
    pub fn eq_value(self, other: Decimal) -> bool {
        self.cmp_value(other) == Ordering::Equal
    }

    /// 加减法：scale 对齐到较大值；溢出返回 `Err`（ADR-006 `checked_add`）。
    pub fn checked_add(self, rhs: Decimal) -> DecimalResult<Decimal> {
        let scale = self.scale.max(rhs.scale);
        let a = self.try_align_scale(scale)?;
        let b = rhs.try_align_scale(scale)?;
        let mantissa =
            a.mantissa.checked_add(b.mantissa).ok_or(DecimalError::RepresentationOverflow)?;
        Decimal { mantissa, scale }.finish()
    }

    /// 减法：scale 对齐到较大值；溢出返回 `Err`（ADR-006 `checked_sub`）。
    pub fn checked_sub(self, rhs: Decimal) -> DecimalResult<Decimal> {
        let scale = self.scale.max(rhs.scale);
        let a = self.try_align_scale(scale)?;
        let b = rhs.try_align_scale(scale)?;
        let mantissa =
            a.mantissa.checked_sub(b.mantissa).ok_or(DecimalError::RepresentationOverflow)?;
        Decimal { mantissa, scale }.finish()
    }

    /// 乘法：mantissa 相乘、scale 相加；溢出返回 `Err`。
    pub fn checked_mul(self, rhs: Decimal) -> DecimalResult<Decimal> {
        let mantissa =
            self.mantissa.checked_mul(rhs.mantissa).ok_or(DecimalError::MantissaOverflow)?;
        let scale = self
            .scale
            .checked_add(rhs.scale)
            .ok_or(DecimalError::ScaleOutOfRange { scale: u8::MAX, max: MAX_SCALE })?;
        Decimal { mantissa, scale }.finish()
    }

    /// 除法：必须显式指定舍入策略（ADR-006 `checked_div`）。
    ///
    /// 结果 scale = `max(self.scale, other.scale)`。
    /// 数值：`round(m1 * 10^(s_r - s1 + s2) / m2)`，等价于对齐双方后再按目标 scale 定点除。
    #[allow(clippy::expect_used)]
    pub fn checked_div(self, other: Decimal, strategy: RoundingStrategy) -> DecimalResult<Decimal> {
        if other.mantissa == 0 {
            return Err(DecimalError::DivisionByZero);
        }
        // 双方 scale ≤ MAX_SCALE ⇒ target_scale ≤ MAX_SCALE
        let target_scale = self.scale.max(other.scale);
        // exp = (target_scale - self.scale) + other.scale ≥ 0
        let exp = u32::from(target_scale - self.scale) + u32::from(other.scale);
        // 不变量：exp ≤ 2*MAX_SCALE，pow10 必落入 i128
        // PANIC: scale 上限保证该幂次可表示；调整上限时须复核。
        let factor = Self::pow10(exp).expect("exp <= 2*MAX_SCALE 保证 10 的幂可由 i128 表示");
        let numerator = self.mantissa.checked_mul(factor).ok_or(DecimalError::MantissaOverflow)?;
        let denominator = other.mantissa;
        // 不变量：i128::MIN / -1 会溢出，故 div 成功（q 为 Some）则 rem 必成功
        let q = numerator.checked_div(denominator).ok_or(DecimalError::MantissaOverflow)?;
        // PANIC: 成功除法已排除 i128::MIN / -1，余数运算不会再溢出。
        let r = numerator.checked_rem(denominator).expect("除法成功时余数运算也应成功");
        let rounded = apply_rounding(q, r, denominator, strategy)?;
        Decimal { mantissa: rounded, scale: target_scale }.finish()
    }

    /// 除法（与 [`Self::checked_div`] 相同；保留既有公开名）。
    pub fn div(self, other: Decimal, strategy: RoundingStrategy) -> DecimalResult<Decimal> {
        self.checked_div(other, strategy)
    }

    /// 显式缩位/扩位到目标 scale（ADR-006）。
    ///
    /// 生产路径请优先使用 [`Self::checked_rescale`]。
    ///
    /// # Panics
    ///
    /// 当 scale 对齐或舍入导致溢出时 panic（内部 `expect`）。禁止用本方法掩盖溢出。
    // 不变量：本方法为显式 panic 便捷封装，溢出即 fail-fast；已提供 checked_rescale 作为非 panic 替代
    #[allow(clippy::expect_used)]
    pub fn rescale(self, target_scale: u8, strategy: RoundingStrategy) -> Decimal {
        // PANIC: 此便捷 API 将 checked_rescale 的错误映射为 panic；调用方可改用 checked_rescale。
        self.checked_rescale(target_scale, strategy).expect("十进制缩放溢出")
    }

    /// 显式缩位/扩位；溢出/非法返回 `Err`。
    #[allow(clippy::expect_used)]
    pub fn checked_rescale(
        self,
        target_scale: u8,
        strategy: RoundingStrategy,
    ) -> DecimalResult<Decimal> {
        if target_scale > MAX_SCALE {
            return Err(DecimalError::ScaleOutOfRange { scale: target_scale, max: MAX_SCALE });
        }
        if target_scale == self.scale {
            return self.finish();
        }
        if target_scale > self.scale {
            return self.try_align_scale(target_scale);
        }
        let diff = u32::from(self.scale - target_scale);
        // 不变量：self.scale ≤ MAX_SCALE 且 target_scale ≥ 0 ⇒ diff ≤ MAX_SCALE，pow10 必落入 i128
        // PANIC: 合法 scale 差值保证该幂次可表示；调整上限时须复核。
        let factor = Self::pow10(diff).expect("diff <= MAX_SCALE 保证 10 的幂可由 i128 表示");
        let q = self.mantissa.checked_div(factor).ok_or(DecimalError::MantissaOverflow)?;
        let r = self.mantissa.checked_rem(factor).ok_or(DecimalError::MantissaOverflow)?;
        let rounded = apply_rounding(q, r, factor, strategy)?;
        Decimal { mantissa: rounded, scale: target_scale }.finish()
    }

    /// 去掉尾随小数零，使 scale 最小且值不变。
    pub fn normalize(self) -> Decimal {
        let mut mantissa = self.mantissa;
        let mut scale = self.scale;
        while scale > 0 && mantissa % 10 == 0 {
            mantissa /= 10;
            scale -= 1;
        }
        Decimal { mantissa, scale }
    }
}

impl PartialEq for Decimal {
    fn eq(&self, other: &Self) -> bool {
        self.eq_value(*other)
    }
}

impl Eq for Decimal {}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        self.cmp_value(*other)
    }
}

impl Hash for Decimal {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let (m, s) = self.canonical();
        m.hash(state);
        s.hash(state);
    }
}

/// 有符号整数 → `Decimal`（`scale = 0`）。`iN` 均落在 `i128` 内，恒成功。
macro_rules! impl_from_signed {
    ($($t:ty),+ $(,)?) => {$(
        impl From<$t> for Decimal {
            #[inline]
            fn from(value: $t) -> Self {
                Self { mantissa: i128::from(value), scale: 0 }
            }
        }
    )+};
}

/// 无符号整数 → `Decimal`（`scale = 0`）。`uN`（`N≤64`）均落在 `i128` 内，恒成功。
macro_rules! impl_from_unsigned {
    ($($t:ty),+ $(,)?) => {$(
        impl From<$t> for Decimal {
            #[inline]
            fn from(value: $t) -> Self {
                Self { mantissa: i128::from(value), scale: 0 }
            }
        }
    )+};
}

impl_from_signed!(i8, i16, i32, i64);
impl_from_unsigned!(u8, u16, u32, u64);

impl From<i128> for Decimal {
    /// 整数转十进制（`scale = 0`）；`i128` 即 mantissa 表示范围，恒成功。
    #[inline]
    fn from(value: i128) -> Self {
        Self { mantissa: value, scale: 0 }
    }
}

impl From<isize> for Decimal {
    /// 整数转十进制（`scale = 0`）。本仓库目标平台 `isize` 宽度 ≤ 64，恒可装入 `i128`。
    #[inline]
    // 不变量：目标平台 isize 宽度 ≤ 64，恒落入 i128 表示范围
    #[allow(clippy::expect_used)]
    fn from(value: isize) -> Self {
        // PANIC: 当前 Rust 目标平台的 isize 不宽于 i128；若目标 ABI 改变须重新验证。
        Self { mantissa: i128::try_from(value).expect("isize 超出 i128 表示范围"), scale: 0 }
    }
}

impl From<usize> for Decimal {
    /// 整数转十进制（`scale = 0`）。本仓库目标平台 `usize` 宽度 ≤ 64，恒可装入 `i128`。
    ///
    /// 消费侧常见：`Decimal::from(slice.len())`。
    #[inline]
    // 不变量：目标平台 usize 宽度 ≤ 64，恒落入 i128 表示范围
    #[allow(clippy::expect_used)]
    fn from(value: usize) -> Self {
        // PANIC: 当前 Rust 目标平台的 usize 不宽于 i128；若目标 ABI 改变须重新验证。
        Self { mantissa: i128::try_from(value).expect("usize 超出 i128 表示范围"), scale: 0 }
    }
}

// ---------------------------------------------------------------------------
// Sum（迭代求和；溢出 panic —— 对齐 panicking-ops 语义，非资金 checked 路径）
// ---------------------------------------------------------------------------

/// 迭代求和：内部走 [`Decimal::checked_add`]。
///
/// **溢出时 panic**（与 feature `panicking-ops` 的 `+` 一致）。
/// 生产资金路径请用 `fold` + [`Decimal::checked_add`] 显式处理错误。
///
/// 供分析域 / 测试侧 `iter.sum::<Decimal>()` 机械迁移。
impl std::iter::Sum for Decimal {
    // 不变量：本 impl 显式对齐 panicking-ops 语义（溢出 fail-fast）；非资金路径，已提供 fold + checked_add 替代
    #[allow(clippy::expect_used)]
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        // PANIC: 该标准 trait 保持显式 fail-fast 求和语义；资金路径应使用 checked_add。
        iter.fold(Self::ZERO, |acc, x| acc.checked_add(x).expect("十进制求和溢出"))
    }
}

/// 引用迭代求和（`iter.copied().sum()` / `iter.sum()` on `&Decimal`）。
impl<'a> std::iter::Sum<&'a Decimal> for Decimal {
    // 不变量：同 `Sum`，显式对齐 panicking-ops 语义（溢出 fail-fast）
    #[allow(clippy::expect_used)]
    fn sum<I: Iterator<Item = &'a Decimal>>(iter: I) -> Self {
        // PANIC: 该标准 trait 保持显式 fail-fast 求和语义；资金路径应使用 checked_add。
        iter.fold(Self::ZERO, |acc, x| acc.checked_add(*x).expect("十进制求和溢出"))
    }
}

// ---------------------------------------------------------------------------
// panicking 运算符（feature = "panicking-ops"；默认关闭）
// ---------------------------------------------------------------------------

/// 加法运算符：内部走 [`Decimal::checked_add`]。
///
/// **仅在 feature `panicking-ops` 下可用。生产资金路径必须用 [`Decimal::checked_add`]。**
///
/// # Panics
///
/// scale 对齐或 mantissa 加法溢出时 panic（含 `MAX_SCALE` 越界）。
#[cfg(feature = "panicking-ops")]
impl std::ops::Add for Decimal {
    type Output = Decimal;
    // 不变量：本 impl 仅在 panicking-ops feature 下编译（默认关闭），显式溢出 fail-fast；生产须用 checked_add
    #[allow(clippy::expect_used)]
    fn add(self, other: Decimal) -> Decimal {
        // PANIC: 此 feature 运算符明确采用溢出即失败语义；资金路径应使用 checked_add。
        self.checked_add(other).expect("十进制加法溢出")
    }
}

/// 减法运算符：内部走 [`Decimal::checked_sub`]。
///
/// **仅在 feature `panicking-ops` 下可用。生产资金路径必须用 [`Decimal::checked_sub`]。**
///
/// # Panics
///
/// scale 对齐或 mantissa 减法溢出时 panic（含 `MAX_SCALE` 越界）。
#[cfg(feature = "panicking-ops")]
impl std::ops::Sub for Decimal {
    type Output = Decimal;
    // 不变量：仅 panicking-ops feature 下编译，显式溢出 fail-fast；生产须用 checked_sub
    #[allow(clippy::expect_used)]
    fn sub(self, other: Decimal) -> Decimal {
        // PANIC: 此 feature 运算符明确采用溢出即失败语义；资金路径应使用 checked_sub。
        self.checked_sub(other).expect("十进制减法溢出")
    }
}

/// 乘法运算符：内部走 [`Decimal::checked_mul`]。
///
/// **仅在 feature `panicking-ops` 下可用。生产资金路径必须用 [`Decimal::checked_mul`]。**
///
/// # Panics
///
/// mantissa 相乘、scale 相加溢出或结果 `scale > MAX_SCALE` 时 panic。
#[cfg(feature = "panicking-ops")]
impl std::ops::Mul for Decimal {
    type Output = Decimal;
    // 不变量：仅 panicking-ops feature 下编译，显式溢出 fail-fast；生产须用 checked_mul
    #[allow(clippy::expect_used)]
    fn mul(self, other: Decimal) -> Decimal {
        // PANIC: 此 feature 运算符明确采用溢出即失败语义；资金路径应使用 checked_mul。
        self.checked_mul(other).expect("十进制乘法溢出")
    }
}

/// 舍入策略（ADR-006）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingStrategy {
    /// 向 −∞ 取整
    Floor,
    /// 向 +∞ 取整
    Ceiling,
    /// 四舍五入（恰好一半时远离零）
    HalfUp,
    /// 五舍六入（恰好一半时趋向零）
    HalfDown,
    /// 银行家舍入（恰好一半时趋向偶数）
    HalfEven,
}

/// 对向零截断的 `(q, r)` 按策略舍入；`r` 为余数（与 Rust `%` 同号于被除数）。
///
/// 中点判定用 `2·|r| ? |d|`，避免 `abs_d/2` 在奇数分母上把非中点当成中点。
fn apply_rounding(q: i128, r: i128, d: i128, strategy: RoundingStrategy) -> DecimalResult<i128> {
    if r == 0 {
        return Ok(q);
    }
    // 真值符号为负 ⟺ 被除数与除数异号。Rust 中 r 与被除数同号。
    let neg = (r < 0) ^ (d < 0);

    let abs_r = r.unsigned_abs();
    let abs_d = d.unsigned_abs();
    // 比较 2·|r| 与 |d|；防止 u128 乘法溢出（|r| > u128::MAX/2）
    let cmp_half = match abs_r.checked_mul(2) {
        Some(two_r) => two_r.cmp(&abs_d),
        None => Ordering::Greater,
    };

    let round_away = match strategy {
        RoundingStrategy::Floor => neg,
        RoundingStrategy::Ceiling => !neg,
        RoundingStrategy::HalfUp => matches!(cmp_half, Ordering::Greater | Ordering::Equal),
        RoundingStrategy::HalfDown => matches!(cmp_half, Ordering::Greater),
        RoundingStrategy::HalfEven => match cmp_half {
            Ordering::Greater => true,
            Ordering::Equal => q % 2 != 0,
            Ordering::Less => false,
        },
    };

    if !round_away {
        return Ok(q);
    }
    if neg {
        q.checked_sub(1).ok_or(DecimalError::RoundingOverflow)
    } else {
        q.checked_add(1).ok_or(DecimalError::RoundingOverflow)
    }
}

// ---------------------------------------------------------------------------
// Decimal serde（校验型 · 跨语言 JSON 交换剖面）
// ---------------------------------------------------------------------------

mod decimal_serde_wire {
    use serde::de::{self, Unexpected, Visitor};
    use serde::{Deserializer, Serializer};
    use std::fmt;

    /// JSON 交换写入：`mantissa` 十进制整数字符串（RFC cross-language wire v1）。
    pub(crate) fn serialize_mantissa<S: Serializer>(
        mantissa: &i128,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&mantissa.to_string())
    }

    /// 读取：接受 string 或历史 JSON integer（只读兼容）。
    pub(crate) fn deserialize_mantissa<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<i128, D::Error> {
        struct MantissaVisitor;
        impl<'de> Visitor<'de> for MantissaVisitor {
            type Value = i128;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("decimal mantissa as decimal integer string or JSON integer")
            }

            fn visit_str<E: de::Error>(self, v: &str) -> Result<i128, E> {
                if v.is_empty() {
                    return Err(de::Error::invalid_value(Unexpected::Str(v), &self));
                }
                v.parse::<i128>().map_err(de::Error::custom)
            }

            fn visit_i64<E: de::Error>(self, v: i64) -> Result<i128, E> {
                Ok(i128::from(v))
            }

            fn visit_u64<E: de::Error>(self, v: u64) -> Result<i128, E> {
                Ok(i128::from(v))
            }

            fn visit_i128<E: de::Error>(self, v: i128) -> Result<i128, E> {
                Ok(v)
            }

            fn visit_f64<E: de::Error>(self, _v: f64) -> Result<i128, E> {
                Err(de::Error::custom("JSON 数字格式的 mantissa 精度不足；请使用十进制整数字符串"))
            }
        }
        deserializer.deserialize_any(MantissaVisitor)
    }
}

struct MantissaStr(i128);

impl Serialize for MantissaStr {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        decimal_serde_wire::serialize_mantissa(&self.0, serializer)
    }
}

impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("Decimal", 2)?;
        st.serialize_field("mantissa", &MantissaStr(self.mantissa))?;
        st.serialize_field("scale", &self.scale)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct DecimalWire {
            #[serde(deserialize_with = "decimal_serde_wire::deserialize_mantissa")]
            mantissa: i128,
            scale: u8,
        }
        let w = DecimalWire::deserialize(deserializer)?;
        Decimal::try_new(w.mantissa, w.scale).map_err(de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;

    fn hash_of(d: Decimal) -> u64 {
        let mut h = DefaultHasher::new();
        d.hash(&mut h);
        h.finish()
    }

    // ── 基础便利 API：From<i64> / ONE / is_zero / abs（rust_decimal → decimalx 收敛面） ──

    #[test]
    fn from_i64_integer() {
        let d = Decimal::from(42);
        assert_eq!(d, Decimal::new(42, 0));
        assert_eq!(d.mantissa(), 42);
        assert_eq!(d.scale(), 0);
    }

    #[test]
    fn from_i64_negative() {
        let d = Decimal::from(-7);
        assert_eq!(d, Decimal::new(-7, 0));
        assert_eq!(d.to_string(), "-7");
    }

    #[test]
    fn one_constant() {
        assert_eq!(Decimal::ONE, Decimal::new(1, 0));
        assert_eq!(Decimal::ONE.to_string(), "1");
    }

    #[test]
    fn ten_and_negative_one_constants() {
        assert_eq!(Decimal::TEN, Decimal::new(10, 0));
        assert_eq!(Decimal::TEN.to_string(), "10");
        assert_eq!(Decimal::NEGATIVE_ONE, Decimal::new(-1, 0));
        assert_eq!(Decimal::NEGATIVE_ONE.to_string(), "-1");
    }

    #[test]
    fn is_zero_detects_zero_across_scales() {
        assert!(Decimal::ZERO.is_zero());
        assert!(Decimal::new(0, 5).is_zero());
        assert!(Decimal::from(0).is_zero());
        assert!(!Decimal::ONE.is_zero());
        assert!(!Decimal::new(3, 1).is_zero());
    }

    #[test]
    fn is_sign_negative_and_positive() {
        assert!(Decimal::NEGATIVE_ONE.is_sign_negative());
        assert!(!Decimal::NEGATIVE_ONE.is_sign_positive());
        assert!(Decimal::ONE.is_sign_positive());
        assert!(!Decimal::ONE.is_sign_negative());
        assert!(!Decimal::ZERO.is_sign_negative());
        assert!(!Decimal::ZERO.is_sign_positive());
        assert!(Decimal::new(-5, 2).is_sign_negative());
    }

    #[test]
    fn abs_returns_non_negative() {
        assert_eq!(Decimal::new(-5, 2).abs(), Ok(Decimal::new(5, 2)));
        assert_eq!(Decimal::new(5, 2).abs(), Ok(Decimal::new(5, 2)));
        assert_eq!(Decimal::ZERO.abs(), Ok(Decimal::ZERO));
        assert_eq!(Decimal::new(-1, 3).abs().unwrap().to_string(), "0.001");
    }

    #[test]
    fn abs_i128_min_returns_overflow_error() {
        // i128::MIN 是合法 mantissa；其绝对值（2^127）无法用 i128 表示。
        // 旧实现 self.mantissa.abs() 在 debug 构建下 panic；修复后显式返回错误。
        let min = Decimal::try_new(i128::MIN, 0).expect("i128::MIN 是合法 mantissa");
        assert_eq!(min.abs(), Err(DecimalError::RepresentationOverflow));
    }

    #[test]
    fn from_i64_roundtrip_through_display() {
        for n in [-100i64, -1, 0, 1, 999999] {
            assert_eq!(Decimal::from(n).to_string(), n.to_string());
        }
    }

    // ── 扩展便利 API：常量 / From 族 / Sum / from_str_exact（kp4 阶段 1.b） ──

    #[test]
    fn numeric_constants() {
        assert_eq!(Decimal::TWO, Decimal::new(2, 0));
        assert_eq!(Decimal::TEN, Decimal::new(10, 0));
        assert_eq!(Decimal::NEGATIVE_ONE, Decimal::new(-1, 0));
        assert_eq!(Decimal::MAX, Decimal::new(i128::MAX, 0));
        assert_eq!(Decimal::MIN, Decimal::new(i128::MIN, 0));
        assert!(Decimal::MIN < Decimal::NEGATIVE_ONE);
        assert!(Decimal::NEGATIVE_ONE < Decimal::ZERO);
        assert!(Decimal::ZERO < Decimal::ONE);
        assert!(Decimal::ONE < Decimal::TWO);
        assert!(Decimal::TWO < Decimal::TEN);
        assert!(Decimal::TEN < Decimal::MAX);
    }

    #[test]
    fn from_integer_family() {
        assert_eq!(Decimal::from(7i8), Decimal::new(7, 0));
        assert_eq!(Decimal::from(-3i16), Decimal::new(-3, 0));
        assert_eq!(Decimal::from(42i32), Decimal::new(42, 0));
        assert_eq!(Decimal::from(100u8), Decimal::new(100, 0));
        assert_eq!(Decimal::from(1000u16), Decimal::new(1000, 0));
        assert_eq!(Decimal::from(1_000_000u32), Decimal::new(1_000_000, 0));
        assert_eq!(Decimal::from(50_000u64), Decimal::new(50_000, 0));
        assert_eq!(Decimal::from(i128::from(99)), Decimal::new(99, 0));
        assert_eq!(Decimal::from(3usize), Decimal::new(3, 0));
        assert_eq!(Decimal::from(-2isize), Decimal::new(-2, 0));
        // 消费侧常见：`Decimal::from(slice.len())`
        let xs = [Decimal::ONE, Decimal::TWO];
        assert_eq!(Decimal::from(xs.len()), Decimal::TWO);
    }

    #[test]
    fn from_str_exact_matches_from_str() {
        assert_eq!(Decimal::from_str_exact("12.5").unwrap(), "12.5".parse().unwrap());
        assert_eq!(Decimal::from_str_exact("-0.01").unwrap().to_string(), "-0.01");
        assert!(Decimal::from_str_exact("nan").is_err());
        assert!(Decimal::from_str_exact("").is_err());
    }

    #[test]
    fn sum_owned_and_borrowed() {
        let vals = [Decimal::from(1), Decimal::from(2), Decimal::from(3)];
        let borrowed: Decimal = vals.iter().sum();
        assert_eq!(borrowed, Decimal::from(6));
        let owned: Decimal = vals.into_iter().sum();
        assert_eq!(owned, Decimal::from(6));
        let empty: Decimal = std::iter::empty::<Decimal>().sum();
        assert!(empty.is_zero());
        // scale 对齐：1 + 0.25 = 1.25
        let mixed = [Decimal::ONE, Decimal::new(25, 2)];
        assert_eq!(mixed.iter().sum::<Decimal>(), Decimal::new(125, 2));
    }

    // ── 加减与 scale 对齐（默认路径用 checked_*；运算符见 panicking-ops） ──

    #[test]
    fn decimal_add_aligns_scale() {
        let a = Decimal::new(1, 0);
        let b = Decimal::new(25, 2);
        let c = a.checked_add(b).unwrap();
        assert_eq!(c.scale(), 2);
        assert_eq!(c.mantissa(), 125);
    }

    #[test]
    fn decimal_sub() {
        let a = Decimal::new(10, 1);
        let b = Decimal::new(1, 0);
        let c = a.checked_sub(b).unwrap();
        assert_eq!(c.mantissa(), 0);
        assert!(c.eq_value(Decimal::ZERO));
    }

    #[test]
    fn decimal_mul_scales_add() {
        let a = Decimal::new(2, 1);
        let b = Decimal::new(3, 0);
        let c = a.checked_mul(b).unwrap();
        assert_eq!(c.scale(), 1);
        assert_eq!(c.mantissa(), 6);
    }

    #[test]
    fn checked_add_sub_mul_concrete() {
        let a = Decimal::new(1, 0);
        let b = Decimal::new(25, 2);
        assert_eq!(a.checked_add(b).unwrap().mantissa(), 125);
        assert_eq!(a.checked_sub(b).unwrap().mantissa(), 75);
        assert_eq!(a.checked_mul(b).unwrap().mantissa(), 25);
    }

    #[cfg(feature = "panicking-ops")]
    #[test]
    fn panicking_ops_match_checked() {
        let a = Decimal::new(1, 0);
        let b = Decimal::new(25, 2);
        assert_eq!(a.checked_add(b).unwrap(), a + b);
        assert_eq!(a.checked_sub(b).unwrap(), a - b);
        assert_eq!(a.checked_mul(b).unwrap(), a * b);
    }

    // ── 相等 / 排序（数值语义，非结构字段） ────────────────────────

    #[test]
    fn equality_aligns_scale() {
        // 1.0 == 1.00 == 1
        assert_eq!(Decimal::new(1, 0), Decimal::new(10, 1));
        assert_eq!(Decimal::new(10, 1), Decimal::new(100, 2));
        assert_eq!(Decimal::new(0, 0), Decimal::new(0, 5));
        assert_ne!(Decimal::new(1, 0), Decimal::new(1, 1));
    }

    #[test]
    fn ordering_aligns_scale() {
        assert!(Decimal::new(1, 0) < Decimal::new(15, 1)); // 1 < 1.5
        assert!(Decimal::new(20, 1) > Decimal::new(1, 0)); // 2.0 > 1
        assert!(Decimal::new(-5, 1) > Decimal::new(-1, 0)); // -0.5 > -1
        assert!(Decimal::new(-20, 1) < Decimal::new(-1, 0)); // -2.0 < -1
    }

    #[test]
    fn hash_consistent_with_value_eq() {
        let a = Decimal::new(1, 0);
        let b = Decimal::new(100, 2);
        assert_eq!(a, b);
        assert_eq!(hash_of(a), hash_of(b));
        assert_eq!(hash_of(Decimal::new(0, 0)), hash_of(Decimal::new(0, 7)));
    }

    // ── 除法 scale 合同 ────────────────────────────────────────────

    #[test]
    fn div_uses_rhs_scale_in_numerator() {
        // 1.00 / 0.50 = 2.00（旧实现把除数 mantissa 当整数，得到 0.02）
        let a = Decimal::new(100, 2);
        let b = Decimal::new(50, 2);
        let c = a.checked_div(b, RoundingStrategy::HalfUp).unwrap();
        assert_eq!(c, Decimal::new(200, 2));
        assert_eq!(c.scale(), 2);
    }

    #[test]
    fn div_mixed_scales() {
        // 10 / 2.5 = 4.0
        let a = Decimal::new(10, 0);
        let b = Decimal::new(25, 1);
        let c = a.div(b, RoundingStrategy::HalfUp).unwrap();
        assert_eq!(c, Decimal::new(40, 1));
    }

    #[test]
    fn decimal_div_half_up_non_midpoint() {
        // 10/3 = 3.333… → scale 0 时 HalfUp 应为 3（非中点）
        let a = Decimal::new(10, 0);
        let b = Decimal::new(3, 0);
        let c = a.div(b, RoundingStrategy::HalfUp).unwrap();
        assert_eq!(c.mantissa(), 3);
        assert_eq!(c.scale(), 0);
    }

    #[test]
    fn decimal_div_floor() {
        let a = Decimal::new(10, 0);
        let b = Decimal::new(3, 0);
        let c = a.div(b, RoundingStrategy::Floor).unwrap();
        assert_eq!(c.mantissa(), 3);
    }

    #[test]
    fn decimal_div_by_zero_errors() {
        let a = Decimal::new(1, 0);
        let b = Decimal::ZERO;
        let err = a.div(b, RoundingStrategy::Floor).unwrap_err();
        assert_eq!(err.kind(), DecimalErrorKind::DivisionByZero);
    }

    #[test]
    fn checked_div_i128_min_over_neg_one_errors_not_panic() {
        // i128::MIN / -1 在 bare `/` 上会 "attempt to divide with overflow"；
        // checked 路径必须返回 Err，与 checked_mul(MIN, -1) 一致。
        let a = Decimal::new(i128::MIN, 0);
        let b = Decimal::new(-1, 0);
        for strategy in [
            RoundingStrategy::Floor,
            RoundingStrategy::Ceiling,
            RoundingStrategy::HalfUp,
            RoundingStrategy::HalfDown,
            RoundingStrategy::HalfEven,
        ] {
            let err = a.checked_div(b, strategy).expect_err("must not panic");
            assert_eq!(err.kind(), DecimalErrorKind::Mantissa, "strategy={strategy:?}: {err}");
            // div 别名同一路径
            assert!(a.div(b, strategy).is_err());
        }
        // 对照：乘法路径对同一边界也返回 Err
        assert!(a.checked_mul(b).is_err());
    }

    // ── Half* 中点与奇数分母 ───────────────────────────────────────

    #[test]
    fn half_up_exact_half_away_from_zero() {
        // 15/2 = 7.5 → HalfUp → 8
        let c = Decimal::new(15, 0).div(Decimal::new(2, 0), RoundingStrategy::HalfUp).unwrap();
        assert_eq!(c.mantissa(), 8);
        // -15/2 = -7.5 → HalfUp → -8
        let c = Decimal::new(-15, 0).div(Decimal::new(2, 0), RoundingStrategy::HalfUp).unwrap();
        assert_eq!(c.mantissa(), -8);
    }

    #[test]
    fn half_down_exact_half_toward_zero() {
        let c = Decimal::new(15, 0).div(Decimal::new(2, 0), RoundingStrategy::HalfDown).unwrap();
        assert_eq!(c.mantissa(), 7);
        let c = Decimal::new(-15, 0).div(Decimal::new(2, 0), RoundingStrategy::HalfDown).unwrap();
        assert_eq!(c.mantissa(), -7);
    }

    #[test]
    fn half_even_ties_to_even() {
        // 15/2 = 7.5 → 向偶数 8
        let c = Decimal::new(15, 0).div(Decimal::new(2, 0), RoundingStrategy::HalfEven).unwrap();
        assert_eq!(c.mantissa(), 8);
        // 13/2 = 6.5 → 向偶数 6
        let c = Decimal::new(13, 0).div(Decimal::new(2, 0), RoundingStrategy::HalfEven).unwrap();
        assert_eq!(c.mantissa(), 6);
        // -13/2 = -6.5 → -6（偶数）
        let c = Decimal::new(-13, 0).div(Decimal::new(2, 0), RoundingStrategy::HalfEven).unwrap();
        assert_eq!(c.mantissa(), -6);
    }

    #[test]
    fn half_up_odd_denominator_not_false_midpoint() {
        // 旧 bug：half = abs_d/2 使 1/3 被当成 ≥ half
        // 10/3 = 3.333… HalfUp → 3；Ceiling → 4
        let a = Decimal::new(10, 0);
        let b = Decimal::new(3, 0);
        assert_eq!(a.div(b, RoundingStrategy::HalfUp).unwrap().mantissa(), 3);
        assert_eq!(a.div(b, RoundingStrategy::Ceiling).unwrap().mantissa(), 4);
        // 2/5 = 0.4 < 0.5 → HalfUp 保持 0
        assert_eq!(
            Decimal::new(2, 0)
                .div(Decimal::new(5, 0), RoundingStrategy::HalfUp)
                .unwrap()
                .mantissa(),
            0
        );
        // 3/5 = 0.6 > 0.5 → HalfUp → 1
        assert_eq!(
            Decimal::new(3, 0)
                .div(Decimal::new(5, 0), RoundingStrategy::HalfUp)
                .unwrap()
                .mantissa(),
            1
        );
    }

    #[test]
    fn floor_ceiling_negative() {
        // -10/3 = -3.333… Floor → -4, Ceiling → -3
        let a = Decimal::new(-10, 0);
        let b = Decimal::new(3, 0);
        assert_eq!(a.div(b, RoundingStrategy::Floor).unwrap().mantissa(), -4);
        assert_eq!(a.div(b, RoundingStrategy::Ceiling).unwrap().mantissa(), -3);
    }

    // ── rescale ────────────────────────────────────────────────────

    #[test]
    fn rescale_expand_and_shrink() {
        let d = Decimal::new(125, 2); // 1.25
        let up = d.rescale(4, RoundingStrategy::HalfUp);
        assert_eq!(up.mantissa(), 12500);
        assert_eq!(up.scale(), 4);

        // 1.25 → scale 1 HalfUp → 1.3
        let down = d.rescale(1, RoundingStrategy::HalfUp);
        assert_eq!(down.mantissa(), 13);
        assert_eq!(down.scale(), 1);

        // 1.25 → scale 1 HalfEven → 1.2（2 为偶数）
        let down_e = d.rescale(1, RoundingStrategy::HalfEven);
        assert_eq!(down_e.mantissa(), 12);
    }

    #[test]
    fn rescale_half_up_midpoint() {
        // 1.25 → scale 1 HalfUp → 1.3；1.35 → 1.4
        assert_eq!(Decimal::new(125, 2).rescale(1, RoundingStrategy::HalfUp).mantissa(), 13);
        assert_eq!(Decimal::new(135, 2).rescale(1, RoundingStrategy::HalfUp).mantissa(), 14);
    }

    // ── 溢出 ──────────────────────────────────────────────────────

    #[test]
    fn align_scale_overflow_errors() {
        let d = Decimal::new(i128::MAX, 0);
        let err = d.try_align_scale(1).unwrap_err();
        assert_eq!(err.kind(), DecimalErrorKind::Representation, "{err}");
    }

    #[test]
    fn checked_add_overflow_errors() {
        let a = Decimal::new(i128::MAX, 0);
        let b = Decimal::new(1, 0);
        assert!(a.checked_add(b).is_err());
    }

    #[test]
    fn checked_mul_overflow_errors() {
        let a = Decimal::new(i128::MAX, 0);
        let b = Decimal::new(2, 0);
        assert!(a.checked_mul(b).is_err());
    }

    #[test]
    fn checked_mul_scale_overflow_errors() {
        let a = Decimal::new(3, 10);
        let b = Decimal::new(1, 10);
        assert!(a.checked_mul(b).is_err());
    }

    #[test]
    fn no_silent_wrap_on_align() {
        // 旧实现 wrapping_mul 会静默回绕得到错误值
        let d = Decimal::new(i128::MAX / 2 + 1, 0);
        assert!(d.try_align_scale(1).is_err());
    }

    #[test]
    fn cmp_value_overflow_safe_magnitude() {
        // 对齐会溢出时仍应正确比较量级
        let huge = Decimal::new(i128::MAX, 0);
        let small = Decimal::new(1, MAX_SCALE); // 极小正数
        assert!(huge > small);
        assert!(Decimal::new(i128::MIN, 0) < small);
    }

    // ── serde round-trip / wire schema v1 稳定合同 ─────────────────

    #[test]
    fn wire_schema_version_is_v1() {
        assert_eq!(WIRE_SCHEMA_VERSION, 1);
    }

    #[test]
    fn wire_schema_v1_decimal_exact_json_shape() {
        // 字段名/顺序/形状冻结：rename 或改 shape 必须先升 WIRE_SCHEMA_VERSION
        let d = Decimal::new(-12345, 3);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, r#"{"mantissa":"-12345","scale":3}"#);
        let back: Decimal = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mantissa(), -12345);
        assert_eq!(back.scale(), 3);
        assert_eq!(back, d);
        // 字段名必须存在（拦截 rename）
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("mantissa").is_some(), "wire v1 requires field mantissa");
        assert_eq!(v["mantissa"].as_str(), Some("-12345"));
        assert!(v.get("scale").is_some(), "wire v1 requires field scale");
        assert_eq!(v.as_object().map(|o| o.len()), Some(2));
    }

    #[test]
    fn wire_schema_v1_decimal_accepts_legacy_integer_mantissa() {
        let legacy = r#"{"mantissa":-12345,"scale":3}"#;
        let d: Decimal = serde_json::from_str(legacy).unwrap();
        assert_eq!(d.mantissa(), -12345);
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, r#"{"mantissa":"-12345","scale":3}"#);
    }

    #[test]
    fn wire_schema_v1_money_exact_json_shape() {
        let m = Money::try_new(Decimal::new(999, 2), "USD".parse().unwrap()).unwrap();
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"amount":{"mantissa":"999","scale":2},"currency":[85,83,68]}"#);
        let back: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(v.get("amount").is_some(), "wire v1 requires field amount");
        assert!(v.get("currency").is_some(), "wire v1 requires field currency");
        assert_eq!(v.as_object().map(|o| o.len()), Some(2));
    }

    #[test]
    fn wire_schema_v1_rejects_renamed_fields() {
        // 若有人把 mantissa 改成 value，这些 golden 反序列化必须失败
        assert!(serde_json::from_str::<Decimal>(r#"{"value":1,"scale":0}"#).is_err());
        assert!(serde_json::from_str::<Decimal>(r#"{"mantissa":1,"precision":0}"#).is_err());
        assert!(
            serde_json::from_str::<Money>(
                r#"{"value":{"mantissa":1,"scale":0},"currency":[85,83,68]}"#
            )
            .is_err()
        );
        assert!(
            serde_json::from_str::<Money>(
                r#"{"amount":{"mantissa":1,"scale":0},"ccy":[85,83,68]}"#
            )
            .is_err()
        );
    }

    #[test]
    fn serde_unknown_fields_rejected_on_decimal() {
        let j = r#"{"mantissa":1,"scale":0,"extra":true}"#;
        let err = serde_json::from_str::<Decimal>(j).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("unknown") || msg.contains("extra"), "{msg}");
    }

    #[test]
    fn serde_roundtrip_struct_fields() {
        let d = Decimal::new(-12345, 3);
        let json = serde_json::to_string(&d).unwrap();
        let back: Decimal = serde_json::from_str(&json).unwrap();
        assert_eq!(back.mantissa(), -12345);
        assert_eq!(back.scale(), 3);
        assert_eq!(back, d);
    }

    #[test]
    fn serde_roundtrip_money() {
        let m = Money::try_new(Decimal::new(999, 2), "USD".parse().unwrap()).unwrap();
        let json = serde_json::to_string(&m).unwrap();
        let back: Money = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
    }

    // ── Currency ───────────────────────────────────────────────────

    #[test]
    fn currency_parse_valid() {
        let c: Currency = "USD".parse().unwrap();
        assert_eq!(c.as_str(), "USD");
    }

    #[test]
    fn currency_parse_invalid() {
        let r: Result<Currency, _> = "us".parse();
        assert!(r.is_err());
        let r: Result<Currency, _> = "USDD".parse();
        assert!(r.is_err());
        let r: Result<Currency, _> = "usd".parse();
        assert!(r.is_err());
    }

    #[test]
    fn no_f32_f64_in_api() {
        let d = Decimal::new(1, 0);
        let _ = (d.mantissa(), d.scale());
    }

    #[test]
    fn from_str_basic() {
        let d: Decimal = "100".parse().unwrap();
        assert_eq!(d, Decimal::new(100, 0));

        let d: Decimal = "100.5".parse().unwrap();
        assert_eq!(d, Decimal::new(1005, 1));

        let d: Decimal = "0".parse().unwrap();
        assert_eq!(d, Decimal::ZERO);

        let d: Decimal = "-1.25".parse().unwrap();
        assert_eq!(d, Decimal::new(-125, 2));

        let d: Decimal = "100.0".parse().unwrap();
        assert_eq!(d, Decimal::new(1000, 1));
        assert!(d.eq_value(Decimal::new(100, 0)));
    }

    #[test]
    fn from_str_rejects_nan_inf_and_garbage() {
        for bad in ["", "nan", "NaN", "inf", "-Inf", "infinity", "1.2.3", "abc", "1e2", "--1", "."]
        {
            let r: Result<Decimal, _> = bad.parse();
            assert!(r.is_err(), "expected err for {bad:?}");
        }
    }

    #[test]
    fn display_strips_trailing_zeros() {
        assert_eq!(Decimal::new(1000, 1).to_string(), "100");
        assert_eq!(Decimal::new(1500, 1).to_string(), "150");
        assert_eq!(Decimal::new(1050, 2).to_string(), "10.5");
        assert_eq!(Decimal::new(0, 2).to_string(), "0");
        assert_eq!(Decimal::new(-125, 2).to_string(), "-1.25");
        assert_eq!(Decimal::new(10, 0).to_string(), "10");
    }

    #[test]
    fn parse_display_roundtrip() {
        for s in ["0", "100", "100.5", "-1.25", "10.5", "999.99", "0.1"] {
            let d: Decimal = s.parse().unwrap();
            let out = d.to_string();
            let back: Decimal = out.parse().unwrap();
            assert!(d.eq_value(back), "roundtrip value mismatch: {s} -> {out} -> {back:?}");
            // Display 已规范化，再次 Display 应稳定
            assert_eq!(back.to_string(), out);
        }
    }

    #[test]
    fn ledger_style_sum_display() {
        // 与 ledger 测试期望对齐： "100.0" + "50.0" → "150"（checked 路径）
        let a: Decimal = "100.0".parse().unwrap();
        let b: Decimal = "50.0".parse().unwrap();
        assert_eq!(a.checked_add(b).unwrap().to_string(), "150");
        let c: Decimal = "10.0".parse().unwrap();
        assert_eq!(c.to_string(), "10");
    }

    // ── M0 边界补强（agent-safe；不宣称全 i128 空间） ──────────────

    #[test]
    fn from_str_leading_trailing_dot_and_trim() {
        let d: Decimal = ".5".parse().unwrap();
        assert_eq!(d, Decimal::new(5, 1));
        let d: Decimal = "5.".parse().unwrap();
        assert_eq!(d, Decimal::new(5, 0));
        let d: Decimal = "  -0.10  ".parse().unwrap();
        assert_eq!(d, Decimal::new(-10, 2));
        assert!(d.eq_value(Decimal::new(-1, 1)));
        let d: Decimal = "+3.0".parse().unwrap();
        assert_eq!(d, Decimal::new(30, 1));
    }

    #[test]
    fn checked_sub_overflow_errors() {
        let a = Decimal::new(i128::MIN, 0);
        let b = Decimal::new(1, 0);
        assert!(a.checked_sub(b).is_err());
    }

    #[test]
    fn checked_rescale_matches_rescale_when_ok() {
        let d = Decimal::new(199, 2); // 1.99
        let c = d.checked_rescale(1, RoundingStrategy::HalfUp).unwrap();
        assert_eq!(c, d.rescale(1, RoundingStrategy::HalfUp));
        assert_eq!(c.mantissa(), 20);
        assert_eq!(c.scale(), 1);
    }

    #[test]
    fn currency_rejects_invalid_bytes_at_construction() {
        assert_eq!(
            Currency::try_new([0xff, 0xfe, 0xfd]).unwrap_err().kind(),
            DecimalErrorKind::Currency
        );
        assert_eq!(Currency::try_new(*b"USD").unwrap().as_str(), "USD");
    }

    #[test]
    #[should_panic(expected = "十进制小数位数超过 MAX_SCALE")]
    fn new_panics_on_scale_above_max() {
        let _ = Decimal::new(1, u8::MAX);
    }

    #[test]
    fn try_new_and_serde_reject_illegal_scale() {
        let err = Decimal::try_new(1, MAX_SCALE + 1).unwrap_err();
        assert_eq!(err.kind(), DecimalErrorKind::Scale);
        let json = format!(r#"{{"mantissa":1,"scale":{}}}"#, MAX_SCALE + 1);
        assert!(serde_json::from_str::<Decimal>(&json).is_err());
        let ok = Decimal::try_new(-12345, 3).unwrap();
        let back: Decimal = serde_json::from_str(&serde_json::to_string(&ok).unwrap()).unwrap();
        assert_eq!(back, ok);
    }

    #[test]
    fn error_kinds_are_distinguishable() {
        assert_eq!(Decimal::try_new(1, 255).unwrap_err().kind(), DecimalErrorKind::Scale);
        assert_eq!(
            Decimal::new(1, 0)
                .checked_div(Decimal::ZERO, RoundingStrategy::Floor)
                .unwrap_err()
                .kind(),
            DecimalErrorKind::DivisionByZero
        );
        assert_eq!(
            Decimal::new(i128::MAX, 0).checked_add(Decimal::new(1, 0)).unwrap_err().kind(),
            DecimalErrorKind::Representation
        );
        assert!(DecimalError::DivisionByZero.to_string().contains("除零"));
        assert!(DecimalError::ScaleOutOfRange { scale: 20, max: 18 }.to_string().contains("scale"));
    }

    #[test]
    fn hash_consistent_negative_trailing_zeros() {
        let a = Decimal::new(-100, 2); // -1.00
        let b = Decimal::new(-10, 1); // -1.0
        let c = Decimal::new(-1, 0); // -1
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(hash_of(a), hash_of(b));
        assert_eq!(hash_of(b), hash_of(c));
    }

    #[test]
    fn serde_shape_is_struct_fields_not_string() {
        let d = Decimal::new(42, 1);
        let json = serde_json::to_string(&d).unwrap();
        assert!(
            json.contains("mantissa") && json.contains("scale"),
            "current wire shape is struct fields: {json}"
        );
        assert!(!json.starts_with('"'), "not a bare decimal string: {json}");
    }

    #[test]
    fn try_new_enforces_max_scale() {
        assert_eq!(MAX_SCALE, 18);
        assert!(Decimal::try_new(1, MAX_SCALE).is_ok());
        assert!(Decimal::try_new(1, MAX_SCALE + 1).is_err());
        assert!(Decimal::new(1, MAX_SCALE).is_within_limits());
    }

    #[test]
    fn from_str_rejects_scale_above_max() {
        let s = format!("0.{}", "1".repeat((MAX_SCALE as usize) + 1));
        assert!(s.parse::<Decimal>().is_err());
        let s_ok = format!("0.{}", "1".repeat(MAX_SCALE as usize));
        assert_eq!(s_ok.parse::<Decimal>().unwrap().scale(), MAX_SCALE);
    }

    #[test]
    fn checked_mul_result_respects_max_scale() {
        assert!(Decimal::new(1, 10).checked_mul(Decimal::new(1, 10)).is_err());
    }

    #[test]
    fn checked_mul_normalizes_before_max_scale_reject() {
        // 1.0 * 1e-18 → 原始 scale 19，normalize 后为 scale 18，应接受
        let a = Decimal::try_new(10, 1).unwrap();
        let b = Decimal::try_new(1, MAX_SCALE).unwrap();
        let p = a.checked_mul(b).expect("exact product within MAX_SCALE after normalize");
        assert_eq!(p, Decimal::try_new(1, MAX_SCALE).unwrap());
        // 不可约 scale 20 仍拒绝
        assert!(
            Decimal::try_new(3, 10).unwrap().checked_mul(Decimal::try_new(1, 10).unwrap()).is_err()
        );
    }

    #[test]
    fn checked_rescale_rejects_target_above_max() {
        assert!(
            Decimal::new(1, 0).checked_rescale(MAX_SCALE + 1, RoundingStrategy::HalfUp).is_err()
        );
    }

    #[test]
    fn currency_try_new_and_money_try_new() {
        let c = Currency::try_new(*b"USD").unwrap();
        assert!(c.is_valid());
        assert!(Currency::try_new(*b"usd").is_err());
        assert!(Money::try_new(Decimal::try_new(100, 2).unwrap(), c).is_ok());
        assert!(Decimal::try_new(1, MAX_SCALE + 1).is_err());
    }

    #[test]
    fn checked_rescale_and_ops_reject_illegal_target() {
        let lo = Decimal::new(1, 0);
        assert!(lo.checked_rescale(MAX_SCALE + 1, RoundingStrategy::HalfUp).is_err());
        let a = Decimal::try_new(1, MAX_SCALE).unwrap();
        let b = Decimal::try_new(1, 0).unwrap();
        let _ = a.checked_add(b);
        assert!(a.checked_div(b, RoundingStrategy::HalfUp).is_ok());
    }

    #[test]
    fn checked_rescale_same_scale_is_identity_finish() {
        let d = Decimal::try_new(123, 2).unwrap();
        let r = d.checked_rescale(2, RoundingStrategy::HalfUp).unwrap();
        assert_eq!(r, d);
    }

    #[test]
    fn from_str_rejects_sign_only_and_non_digit_frac() {
        assert!("-".parse::<Decimal>().is_err());
        assert!("+".parse::<Decimal>().is_err());
        assert!("1.2x".parse::<Decimal>().is_err());
        assert!("1.2.3".parse::<Decimal>().is_err());
        assert!("abc".parse::<Decimal>().is_err());
    }

    #[test]
    fn money_and_currency_validate_surface() {
        let ok =
            Money::try_new(Decimal::try_new(1, 2).unwrap(), Currency::try_new(*b"USD").unwrap())
                .unwrap();
        assert!(ok.validate().is_ok());
        assert_eq!(ok.amount().mantissa(), 1);
        assert_eq!(ok.currency().as_str(), "USD");
        assert!(Currency::try_new(*b"usd").is_err());
        let bad_json = r#"{"amount":{"mantissa":1,"scale":0},"currency":[117,115,100]}"#;
        assert!(serde_json::from_str::<Money>(bad_json).is_err());
    }

    #[test]
    fn checked_add_rhs_align_fails_when_lhs_already_at_target() {
        // a 已在目标 scale；b 对齐时 mantissa * 10^diff 溢出 → 仅 b 侧失败
        let a = Decimal::new(1, MAX_SCALE);
        let b = Decimal::new(i128::MAX, 0);
        assert!(a.checked_add(b).is_err());
        assert!(a.checked_sub(b).is_err());
    }

    #[test]
    fn apply_rounding_edges_and_overflow() {
        // r==0 快路径
        assert_eq!(apply_rounding(7, 0, 3, RoundingStrategy::HalfUp).unwrap(), 7);
        // |r| 极大 → checked_mul(2) 溢出分支
        let huge_r = i128::MIN; // unsigned_abs = 2^127 > u128::MAX/2
        let _ = apply_rounding(0, huge_r, -2, RoundingStrategy::HalfUp);
        // q 在 i128 边界时 round-away 溢出
        assert!(apply_rounding(i128::MAX, 1, 2, RoundingStrategy::Ceiling).is_err());
        assert!(apply_rounding(i128::MIN, -1, 2, RoundingStrategy::Floor).is_err());
    }

    #[test]
    fn from_str_mantissa_overflow_errors() {
        // 远超 i128 的数字串
        let huge = format!("9{}", "0".repeat(50));
        assert!(huge.parse::<Decimal>().is_err());
    }

    #[test]
    fn div_with_high_scales_hits_pow10_none_when_possible() {
        // 在合法 MAX_SCALE 内，exp 最大 2*18=36，pow10 仍成功；此测锁定不 panic
        let a = Decimal::try_new(1, MAX_SCALE).unwrap();
        let b = Decimal::try_new(1, MAX_SCALE).unwrap();
        let _ = a.checked_div(b, RoundingStrategy::HalfUp);
    }

    #[test]
    fn checked_div_rem_overflow_path() {
        // i128::MIN / -1 的 rem/div 溢出已有测；再覆盖 apply_rounding 成功路径后 finish
        let a = Decimal::new(i128::MIN, 0);
        let b = Decimal::new(-1, 0);
        assert!(a.checked_div(b, RoundingStrategy::HalfUp).is_err());
    }

    #[test]
    fn try_align_scale_rejects_target_above_max() {
        let err = Decimal::new(1, 0).try_align_scale(MAX_SCALE + 1).unwrap_err();
        assert_eq!(err.kind(), DecimalErrorKind::Scale);
    }

    #[test]
    fn newtype_accessors_and_currency_bytes() {
        let d = Decimal::try_new(125, 2).unwrap();
        let price = Price::new(d);
        assert_eq!(price.as_decimal(), d);
        assert_eq!(price.into_inner(), d);
        let qty = Qty::new(d);
        assert_eq!(qty.as_decimal(), d);
        assert_eq!(qty.into_inner(), d);
        let ratio = Ratio::new(d);
        assert_eq!(ratio.as_decimal(), d);
        assert_eq!(ratio.into_inner(), d);
        let c = Currency::try_new(*b"USD").unwrap();
        assert_eq!(c.as_bytes(), *b"USD");
        assert_eq!(c.as_str(), "USD");
        // Display 负小数 + 正小数路径
        assert_eq!(Decimal::try_new(-105, 2).unwrap().to_string(), "-1.05");
        assert_eq!(Decimal::try_new(105, 2).unwrap().to_string(), "1.05");
    }

    #[test]
    fn decimal_error_kinds_and_xerror_from() {
        use kernel::XError;
        for e in [
            DecimalError::MantissaOverflow,
            DecimalError::RoundingOverflow,
            DecimalError::RepresentationOverflow,
            DecimalError::Parse("x".into()),
            DecimalError::InvalidCurrency,
        ] {
            let _ = e.kind();
            let s = e.to_string();
            assert!(!s.is_empty());
            let xe: XError = e.into();
            assert!(!xe.context().is_empty());
        }
    }
}
