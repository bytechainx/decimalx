# 项目上下文

`decimalx` 是精确十进制与金额值对象 crate，当前版本 `0.1.8`。

- 语义权威：[`docs/标准.md`](docs/标准.md)。
- 公开 API：[`docs/API.md`](docs/API.md)；序列化格式：[`docs/WIRE.md`](docs/WIRE.md)。
- `kernel` 是唯一内部 crate 依赖，必须保持 `version + path` 与版本一致。
- 资金路径使用 checked API；`panicking-ops` 默认关闭；禁止以浮点类型表示金额。
- Rust Edition 2024，MSRV 1.88；通过 Git checkout 消费，不发布到 crates.io。
