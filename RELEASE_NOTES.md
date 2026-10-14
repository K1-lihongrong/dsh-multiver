## DSH 版本管理器 v0.2.12

### 🧹 技术债清理（卫生补丁）

本次为**无功能变更**的清理版本，对用户完全透明。v0.2.9~v0.2.11 连续三个功能版本后，做一次"打扫房间"。

**代码质量**

- 清掉全部 10 条 Clippy 警告（collapsible_if / doc_lazy_continuation / ptr_arg / derivable_impls / io_other_error / unnecessary_cast）。
- 全量 `cargo fmt` 格式化（约 2500 行 diff，单独提交，便于 git blame 跳过）。

**工程实践**

- CI 增加 lint 门禁：`cargo clippy -- -D warnings` + `cargo fmt --check`，防止未来再累积。

**代码结构**

- `useI18n.js` 拆分：958 → 91 行，4 语言词条拆到 `src/composables/i18n/{zh-CN,zh-TW,en-US,ja-JP}.js`。
- 词条内容逐条核对一致（key 集合 + value 全等），4 语言渲染验证通过。

### 说明

- **无破坏性变更**：所有输出、行为、语义均不变
- 界面、CLI 输出与 v0.2.11 完全一致

---

完整历史见 [CHANGELOG.md](https://github.com/K1-lihongrong/dsh-multiver/blob/main/CHANGELOG.md)。
