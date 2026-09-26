# 变更记录

本仓库从独立 Git 源码版本 `0.1.8` 开始维护，不发布到 crates.io。

## [Unreleased]

- 将本体角色登记为 `core`，与基础值仓的 008 名册一致；公开 API 不变。

## [0.1.8] - 2026-09-23 — bytechainx 初始迁移

### 说明

- 从 `xhyper.rs` 迁移 decimalx 实现、测试、示例和基准代码，保留 crate 版本、API 与 wire 结构。
- 将原工作区共享的 wire fixture 纳入仓库，保持测试可独立运行。
- `panicking-ops` 仍默认关闭。
