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
  const m = msg.match(/diagnostics:\s*(.+)/);
  if (m) extra.push(t("dsplugins.diagnosticsHint", { p: m[1].trim() }));
  return extra.length ? msg + "\n\n" + extra.join("\n") : msg;
}
