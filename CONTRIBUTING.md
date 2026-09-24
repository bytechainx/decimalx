# 贡献指南

1. 在 feature branch 上提交变更，并通过 Pull Request 合入 `main`。
2. 修改数值边界或 wire 格式前，先更新 [`docs/标准.md`](docs/标准.md) 与 [`docs/WIRE.md`](docs/WIRE.md)。
3. 运行 `cargo fmt --check`、`cargo clippy --all-targets -- -D warnings` 和 `cargo test`。
4. 金额与数量运算必须使用精确十进制和 checked API；默认关闭 panicking 运算符。
