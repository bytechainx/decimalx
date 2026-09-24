#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! 最小消费者路径：checked 四则 + Money/Currency。
//!
//! ```bash
//! cargo run -p decimalx --example decimalx_basic
//! DECIMALX_LIVE_PROFILE=production cargo run -p decimalx --example decimalx_basic
//! ```

use decimalx::{Currency, Decimal, Money, Price, Qty, Ratio, RoundingStrategy};
use std::env;
use std::str::FromStr;

fn profile_label() -> &'static str {
    match env::var("DECIMALX_LIVE_PROFILE").as_deref() {
        Ok("production") => "production",
        _ => "development",
    }
}

fn main() {
    let a = Decimal::try_new(10, 0).expect("try_new");
    let b = Decimal::from_str("3").expect("parse");
    let sum = a.checked_add(b).expect("add");
    assert_eq!(sum.mantissa(), 13);

    let q = a.checked_div(b, RoundingStrategy::HalfEven).expect("div");
    assert!(q.mantissa() != 0);

    let ccy = Currency::try_new(*b"USD").expect("currency");
    let money = Money::try_new(a, ccy).expect("money");
    assert_eq!(money.currency().as_str(), "USD");
    assert_eq!(money.amount().mantissa(), 10);

    let price = Price::new(Decimal::try_new(105, 2).expect("price dec"));
    let qty = Qty::new(Decimal::try_new(25, 1).expect("qty dec"));
    let ratio = Ratio::new(Decimal::try_new(15, 2).expect("ratio dec"));

    let json = serde_json::to_string(&money).expect("serialize");
    let back: Money = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back.amount(), money.amount());

    println!(
        "decimalx-consumer: ok profile={} sum={} quot_mantissa={} money_usd={} price={} qty={} ratio={}",
        profile_label(),
        sum,
        q.mantissa(),
        money.amount(),
        price.as_decimal().mantissa(),
        qty.as_decimal().mantissa(),
        ratio.as_decimal().mantissa(),
    );
}
