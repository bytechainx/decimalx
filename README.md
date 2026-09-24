# decimalx

`decimalx` 提供精确十进制、货币、价格和数量值对象，以及带溢出检查的算术操作。

| 项目 | 值 |
| --- | --- |
| 版本 | `0.1.8` |
| Rust | Edition 2024，MSRV 1.88 |
| 许可 | MIT |
| 发布 | 仅从 Git 源码消费，不发布到 crates.io |

## 获取源码

内部依赖采用 `path + version`。将依赖仓库并排克隆，使 `kernel` 路径依赖可解析：

```bash
git clone git@github.com:bytechainx/kernel.git
git clone git@github.com:bytechainx/decimalx.git
```

在同一目录结构中配置：

```toml
[dependencies]
decimalx = { version = "0.1.8", path = "../decimalx" }
```

## 主要类型

- `Decimal`：精确十进制值，最多 18 位小数。
- `Currency`、`Money`：货币与金额值对象。
- `Price`、`Qty`、`Ratio`：价格、数量和比例 newtype。
- `RoundingStrategy`：Floor、Ceiling、HalfUp、HalfDown、HalfEven。

资金路径使用 `checked_add`、`checked_sub`、`checked_mul`、`checked_div` 和 `checked_rescale`。`panicking-ops` 默认关闭；不使用 `f32` 或 `f64` 表示金额。

## 验证

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```
