# wlsnap 项目规则

## 依赖管理

- **Rust crate 依赖只能通过命令行 `cargo add` 管理，禁止直接编辑 `Cargo.toml` 文件。**
- 若需指定 features，使用 `cargo add <crate> --features <feature1>,<feature2>`。
- 若需指定版本，使用 `cargo add <crate>@<version>`。
- `Cargo.toml` 中 `[package]` 段的基础元信息（name, version, edition）除外，可在初始化时配置。

## 文档维护

- 修改代码时，同步更新 `docs/dev/` 下对应的设计文档。
- 技术选型变更时，在 `docs/dev/02-design.md` 的 ADR 章节记录决策。
