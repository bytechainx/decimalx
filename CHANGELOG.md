# 变更记录

本仓库从独立 Git 源码版本 `0.1.8` 开始维护，不发布到 crates.io。

## [Unreleased]

### 变更

- `kernel` 依赖由 `path = "../kernel"` 改为 Git 依赖（`rev = 2b4e8422`，与 workspace lock 对齐）；只克隆本仓即可构建，CI 不再检出兄弟仓。公开 API 不变。

## [0.1.9] - 2026-09-27

- 将本体角色登记为 `kernel`，与 Feature 008 冻结名册一致；公开 API 不变。
- 将 `kernel` 依赖声明对齐 `0.5.0`（时间面迁出）。

## [0.1.8] - 2026-09-23 — bytechainx 初始迁移

### 说明

- 从 `xhyper.rs` 迁移 decimalx 实现、测试、示例和基准代码，保留 crate 版本、API 与 wire 结构。
- 将原工作区共享的 wire fixture 纳入仓库，保持测试可独立运行。
- `panicking-ops` 仍默认关闭。
