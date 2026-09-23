//! `Decimal` 周边的价格、数量、比率、币种与金额值对象。

use super::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

/// 价格（newtype，spec §4.2）。内部值私有，仅能包裹已校验 [`Decimal`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Price(Decimal);

impl Price {
    /// 由已校验 [`Decimal`] 构造。
    pub const fn new(d: Decimal) -> Self {
        Self(d)
    }
    /// 查看内部十进制值。
    pub const fn as_decimal(self) -> Decimal {
        self.0
    }
    /// 取出内部十进制值。
    pub const fn into_inner(self) -> Decimal {
        self.0
    }
}

/// 数量（newtype，spec §4.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Qty(Decimal);

impl Qty {
    /// 由已校验 [`Decimal`] 构造。
    pub const fn new(d: Decimal) -> Self {
        Self(d)
    }
    /// 查看内部十进制值。
    pub const fn as_decimal(self) -> Decimal {
        self.0
    }
    /// 取出内部十进制值。
    pub const fn into_inner(self) -> Decimal {
        self.0
    }
}

/// 比率（newtype，spec §4.2）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ratio(Decimal);

impl Ratio {
    /// 由已校验 [`Decimal`] 构造。
    pub const fn new(d: Decimal) -> Self {
        Self(d)
    }
    /// 查看内部十进制值。
    pub const fn as_decimal(self) -> Decimal {
        self.0
    }
    /// 取出内部十进制值。
    pub const fn into_inner(self) -> Decimal {
        self.0
    }
}

/// ISO 4217 风格币种标识（3 字节大写 ASCII）。字段私有；仅合法币种可构造。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Currency([u8; 3]);

impl Currency {
    /// 将内部 3 字节解释为 UTF-8（构造不变量保证合法大写 ASCII）。
    // 不变量：Currency 字段私有，仅 try_new / Deserialize 校验大写 ASCII 后构造，恒为合法 UTF-8
    #[allow(clippy::expect_used)]
    pub fn as_str(&self) -> &str {
        // PANIC: 私有字段只允许通过大写 ASCII 校验构造，故 UTF-8 转换恒成功。
        std::str::from_utf8(&self.0).expect("currency invariant: uppercase ASCII")
    }

    /// 原始三字节。
    pub const fn as_bytes(self) -> [u8; 3] {
        self.0
    }

    /// 生产构造：三字节均须为大写 ASCII 字母。
    pub fn try_new(bytes: [u8; 3]) -> DecimalResult<Self> {
        if !bytes.iter().all(|c| c.is_ascii_uppercase()) {
            return Err(DecimalError::InvalidCurrency);
        }
        Ok(Self(bytes))
    }

    /// 当前字节是否全部为大写 ASCII（构造成功后恒 true）。
    pub fn is_valid(self) -> bool {
        self.0.iter().all(|c| c.is_ascii_uppercase())
    }

    /// 校验后返回自身，否则 `Err`。
    pub fn validate(self) -> DecimalResult<Self> {
        Self::try_new(self.0)
    }
}

impl Serialize for Currency {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Currency {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = <[u8; 3]>::deserialize(deserializer)?;
        Currency::try_new(bytes).map_err(de::Error::custom)
    }
}

impl std::str::FromStr for Currency {
    type Err = DecimalError;

    fn from_str(s: &str) -> DecimalResult<Self> {
        let b = s.as_bytes();
        if b.len() != 3 {
            return Err(DecimalError::InvalidCurrency);
        }
        let mut arr = [0u8; 3];
        arr.copy_from_slice(b);
        Self::try_new(arr)
    }
}

/// 金额（spec §4.2）。字段私有；生产请用 [`Money::try_new`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Money {
    amount: Decimal,
    currency: Currency,
}

impl Money {
    /// 生产构造：同时校验 amount scale 与 currency 合法性。
    pub fn try_new(amount: Decimal, currency: Currency) -> DecimalResult<Self> {
        let amount = amount.validate()?;
        let currency = currency.validate()?;
        Ok(Self { amount, currency })
    }

    /// 金额数值。
    pub const fn amount(self) -> Decimal {
        self.amount
    }

    /// 币种。
    pub const fn currency(self) -> Currency {
        self.currency
    }

    /// 校验 amount/currency 后返回自身。
    pub fn validate(self) -> DecimalResult<Self> {
        Self::try_new(self.amount, self.currency)
    }
}

impl Serialize for Money {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = serializer.serialize_struct("Money", 2)?;
        st.serialize_field("amount", &self.amount)?;
        st.serialize_field("currency", &self.currency)?;
        st.end()
    }
}

impl<'de> Deserialize<'de> for Money {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct MoneyWire {
            amount: Decimal,
            currency: Currency,
        }
        let w = MoneyWire::deserialize(deserializer)?;
        Money::try_new(w.amount, w.currency).map_err(de::Error::custom)
    }
}
