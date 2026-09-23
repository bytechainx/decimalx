#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unreachable)]
//! decimalx 热路径：checked 四则。
use std::hint::black_box;
use std::time::Instant;

use decimalx::{Decimal, RoundingStrategy};

fn iters() -> u32 {
    if std::env::args().any(|a| a == "--quick") { 5_000 } else { 500_000 }
}

fn main() {
    let n = iters();
    let a = Decimal::new(123456789, 4);
    let b = Decimal::new(987654321, 6);
    for _ in 0..n.min(50) {
        let _ = black_box(a.checked_add(b));
    }
    let start = Instant::now();
    let mut acc = 0i128;
    for _ in 0..n {
        let s = a.checked_add(b).expect("add");
        let d = s.checked_sub(b).expect("sub");
        let m = d.checked_mul(b).expect("mul");
        let q = m.checked_div(b, RoundingStrategy::HalfEven).expect("div");
        acc = acc.wrapping_add(q.mantissa());
        black_box(q);
    }
    let elapsed = start.elapsed();
    println!(
        "bench_decimalx_ops: iters={n} total={elapsed:?} per_iter={:?} acc={}",
        elapsed / n,
        black_box(acc)
    );
}
