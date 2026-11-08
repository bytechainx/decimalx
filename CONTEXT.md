# 项目上下文

`decimalx` 是精确十进制与金额值对象 crate，当前版本 `0.1.9`。

- 行为合同唯一权威：元仓 `specs/repos/type/decimalx` 的 `SPEC.md` 与 `specs/01..10`（Owner 裁定 2026-11-05）；本仓 [`docs/标准.md`](docs/标准.md) 为派生摘要。
- 公开 API：[`docs/API.md`](docs/API.md)；序列化格式：[`docs/WIRE.md`](docs/WIRE.md)。
- `kernel` 是唯一内部 crate 依赖，以 `version` + `git` + `rev` 声明（`rev` 与 workspace `crates.lock.json` 登记的 SHA 对齐）。
- 资金路径使用 checked API；`panicking-ops` 默认关闭；禁止以浮点类型表示金额。
- Rust Edition 2024，MSRV 1.88；通过 Git checkout 消费，不发布到 crates.io。
