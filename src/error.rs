//! 十进制运算与构造错误类型及其分类。

use kernel::XError;
use std::fmt;

// ---------------------------------------------------------------------------
// DecimalError
// ---------------------------------------------------------------------------

/// 十进制运算与构造错误（可分类；用户可见 `Display` 为中文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecimalError {
    /// scale 超出 [`MAX_SCALE`]。
    ScaleOutOfRange {
        /// 实际 scale。
        scale: u8,
        /// 允许的最大 scale（[`MAX_SCALE`]）。
        max: u8,
    },
    /// mantissa 解析或运算溢出。
    MantissaOverflow,
    /// 除数为零。
    DivisionByZero,
    /// 舍入步进导致溢出。
    RoundingOverflow,
    /// 表示范围不足（对齐/中间值等）。
    RepresentationOverflow,
    /// 解析失败。
    Parse(String),
    /// 币种非法。
    InvalidCurrency,
}

impl DecimalError {
    /// 错误分类（便于 match，不依赖字符串）。
    pub fn kind(&self) -> DecimalErrorKind {
        match self {
            Self::ScaleOutOfRange { .. } => DecimalErrorKind::Scale,
            Self::MantissaOverflow => DecimalErrorKind::Mantissa,
            Self::DivisionByZero => DecimalErrorKind::DivisionByZero,
            Self::RoundingOverflow => DecimalErrorKind::Rounding,
            Self::RepresentationOverflow => DecimalErrorKind::Representation,
            Self::Parse(_) => DecimalErrorKind::Parse,
            Self::InvalidCurrency => DecimalErrorKind::Currency,
        }
    }
}

/// [`DecimalError`] 的稳定分类标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DecimalErrorKind {
    /// scale 越界
    Scale,
    /// mantissa 溢出
    Mantissa,
    /// 除零
    DivisionByZero,
    /// 舍入溢出
    Rounding,
    /// 表示/中间值范围
    Representation,
    /// 解析
    Parse,
    /// 币种
    Currency,
}

impl fmt::Display for DecimalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ScaleOutOfRange { scale, max } => {
                write!(f, "十进制 scale {scale} 超过上限 {max}")
            }
            Self::MantissaOverflow => write!(f, "十进制 mantissa 溢出"),
            Self::DivisionByZero => write!(f, "十进制除零"),
            Self::RoundingOverflow => write!(f, "十进制舍入溢出"),
            Self::RepresentationOverflow => write!(f, "十进制表示范围不足（中间值溢出）"),
            Self::Parse(msg) => write!(f, "十进制解析失败: {msg}"),
            Self::InvalidCurrency => write!(f, "币种必须为 3 个大写 ASCII 字母"),
        }
    }
}

impl std::error::Error for DecimalError {}

impl From<DecimalError> for XError {
    fn from(err: DecimalError) -> Self {
        let context = err.to_string();
        XError::invalid(context).with_source(err)
    }
}
