import { t } from "./useI18n.js";
import { errorDisplayText, isApiError, toApiError } from "../api/errors.js";

// 官方 CLI 的失败诊断对齐：把官方提示（pnpm 缺失 / 诊断日志路径）转成本地化提示。
// DsPluginPanel 的壳与各子组件共用。
//
// 版本不兼容：dsh 0.2.2-rc.1 已提供 allow-version / revoke-version /
// version-exemptions 子命令，由 dsh 自己拦截，不会转发给 pnpm。
// 这里只负责从 CLI 输出解析出可执行的精确版本豁免项。
/// 从 dsh/pnpm 输出中提取需要精确版本豁免的插件。
/// 兼容两类行：
/// - Plugin pkg@ver is incompatible with dsh runtime:
/// - dsh: to accept the risk, run: dsh plugin --profile p allow-version pkg@ver --dsh-version runtime --accept-risk
export function parseIncompatiblePlugins(text) {
  const out = [];
  const seen = new Set();
  const re = /Plugin\s+(@?[^\s@]+(?:\/[^\s@]+)?@[^\s]+)\s+is incompatible with dsh\s+([^\s:]+):/g;
  let m;
  while ((m = re.exec(String(text || ""))) !== null) {
    const key = m[1] + "|" + m[2];
    if (seen.has(key)) continue;
    seen.add(key);
    out.push({ packageVersion: m[1], dshVersion: m[2] });
  }
  return out;
}

export function actionErrorText(e) {
  const structured = toApiError(e);
  const isStructured = isApiError(e);
  const msg = isStructured ? errorDisplayText(e) : String(e || "");
  const extra = [];
  if (msg.includes("pnpm was not found")) extra.push(t("dsplugins.pnpmMissingHint"));
  // 包已不在 dependencies 里，但仍留在 profile 的 bundles 列表。
  // 此时 pnpm 拒绝卸载，需要用户手动清理 profile 配置。
  if (msg.includes("ERR_PNPM_CANNOT_REMOVE_MISSING_DEPS")) {
    extra.push(t("dsplugins.missingDepHint"));
  }
  const m = msg.match(/diagnostics:\s*(.+)/);
  if (m) extra.push(t("dsplugins.diagnosticsHint", { p: m[1].trim() }));
  // 结构化错误若带有原始输出，保留在提示末尾（插件分类卡片另有专用复制按钮）。
  if (isStructured && structured.raw && !msg.includes(structured.raw)) {
    extra.push(structured.raw);
  }
  return extra.length ? msg + "\n\n" + extra.join("\n") : msg;
}
