# decimalx Wire / Storage 边界（生产政策）

**Status**：Policy · wire schema **v1**（字段形状冻结；非 crates.io package stable）  
**Package**：`decimalx` 0.1.x  
**常量**：[`WIRE_SCHEMA_VERSION`](../src/lib.rs) = `1`

| 边界 | 当前事实 | 稳定承诺 |
|------|----------|----------|
| serde `Decimal` | `{mantissa, scale}`；**写入** `mantissa` 为十进制字符串；**读取**兼容历史 JSON 整数；反序列化走 `try_new`；`deny_unknown_fields` | **v1** 字段名与 shape；同 major 内扩展；破坏性变更须升 `WIRE_SCHEMA_VERSION` |
| serde `Currency` | 3 字节数组 `[u8;3]`（大写 ASCII） | **v1** |
| serde `Money` | `{amount, currency}`；`deny_unknown_fields` | **v1** |
| Display/FromStr | 文本；FromStr 强制 `scale≤MAX_SCALE(18)` | 非持久化协议 |
| SQL NUMERIC | 由上层 adapter/schema 映射 | 非本 crate 合同 |
| 中间值能力 | `i128` 溢出 → `Err`（即使约分后可表示） | 正式合同，非 silent bug |

生产规则：

1. 金额入口 `try_new` / `parse` / `validate`；运算 **仅** `checked_*`。
2. 资金路径禁用 panicking `+` / `-` / `*` / `rescale`；默认关闭 `panicking-ops` feature。
3. panicking 运算符 `impl Add/Sub/Mul` 仅在 feature **`panicking-ops`** 下公开，**默认关闭**。
4. 不得仅凭 derive 或 100% 行覆盖宣称 package stable / crates.io Production Ready。
5. 正确性证据：`tests/oracle_diff.rs`（BigDecimal 差分，仅 `Ok` 路径）+ `tests/boundary_matrix.rs` + unit `wire_schema_v1_*` + scheduled mutants/miri。
6. 字段 rename / shape 破坏必须：(a) 升 `WIRE_SCHEMA_VERSION`；(b) 更新本文件与 golden 测试；否则 `wire_schema_v1_*` 失败。
