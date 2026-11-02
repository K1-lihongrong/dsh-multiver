import { t } from "./useI18n.js";

// 官方 CLI 的失败诊断对齐：把官方提示（allow-version / pnpm 缺失 / 诊断日志路径）
// 转成本地化提示。DsPluginPanel 的壳与各子组件共用。
export function actionErrorText(e) {
  const msg = String(e || "");
  const extra = [];
  if (msg.includes("allow-version") || msg.includes("incompatible-version")) {
    extra.push(t("dsplugins.incompatibleHint"));
  }
  if (msg.includes("pnpm was not found")) extra.push(t("dsplugins.pnpmMissingHint"));
  // 包已不在 dependencies 里，但仍留在 profile 的 bundles 列表。
  // 此时 pnpm 拒绝卸载，需要用户手动清理 profile 配置。
  if (msg.includes("ERR_PNPM_CANNOT_REMOVE_MISSING_DEPS")) {
    extra.push(t("dsplugins.missingDepHint"));
  }
  const m = msg.match(/diagnostics:\s*(.+)/);
  if (m) extra.push(t("dsplugins.diagnosticsHint", { p: m[1].trim() }));
  return extra.length ? msg + "\n\n" + extra.join("\n") : msg;
}
