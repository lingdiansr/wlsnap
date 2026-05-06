# wlsnap 文档中心

> 本文档目录包含 wlsnap 项目的全部技术文档，按读者角色分层组织。

---

## 目录结构

```
docs/
├── README.md              # 本文档
├── dev/                   # 开发者文档
│   ├── 01-tech-spec.md    # 技术选型方案
│   ├── 02-design.md       # 详细架构设计
│   └── 03-roadmap.md      # 开发路线图与任务排期
├── user/                  # 用户文档（待补充）
│   ├── README.md          # 使用说明（计划）
│   └── config-guide.md    # 配置指南（计划）
└── project/               # 项目元信息（待补充）
    ├── CHANGELOG.md       # 更新日志（计划）
    └── CONTRIBUTING.md    # 贡献指南（计划）
```

---

## 阅读指引

### 我是用户，想了解如何使用

请阅读 `user/` 目录下的文档（待补充）。

### 我是开发者，想参与开发或了解实现

建议按以下顺序阅读：

1. [`dev/01-tech-spec.md`](dev/01-tech-spec.md) — 技术选型方案
   - 了解为什么选择这些技术栈
   - 各桌面环境的兼容性矩阵
   - Rust 依赖清单

2. [`dev/02-design.md`](dev/02-design.md) — 详细架构设计
   - 模块划分与目录结构
   - 核心 Trait 与类型定义
   - 状态机、事件循环、数据流
   - 配置系统、CLI 设计、错误处理
   - 边界条件与测试策略

3. [`dev/03-roadmap.md`](dev/03-roadmap.md) — 开发路线图与排期
   - Phase 1~4 的任务分解
   - 任务依赖关系与并行策略
   - 里程碑检查点
   - 工作量估算

### 配置文件示例

参考项目根目录下的 [`config/config.example.toml`](../config/config.example.toml)。

---

## 文档维护

- 修改代码时，请同步更新对应的设计文档。
- 新增功能前，先在 `dev/03-roadmap.md` 中登记任务。
- 技术选型变更时，同步更新 `dev/01-tech-spec.md` 并在 `dev/02-design.md` 的 ADR 章节记录决策。
