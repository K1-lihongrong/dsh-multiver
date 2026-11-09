// A2-8: 前端统一错误分类。
// 后端命令仍可能以字符串拒绝；插件安装/更新失败由
// ds_classify_plugin_failure 返回 { code, message, detail, raw, log_path }。
// 这里把两种形态归一化成 ErrorKind + {code,message,detail,raw?}，
// 任何分支都保留 raw，不替换 pnpm/dsh 原始输出。
export const ErrorKind = Object.freeze({
  versionIncompatible: "version_incompatible",
  releaseTooNew: "release_too_new",
  fileLocked: "file_locked",
  network: "network",
  wrongEnvironment: "wrong_environment",
  peerWarning: "peer_warning",
  buildBlocked: "build_blocked",
  unknown: "unknown",
  commandFailed: "command_failed",
  conflict: "conflict",
});

const KNOWN_CODES = new Set(Object.values(ErrorKind));
const PLUGIN_CODES = new Set([
  ErrorKind.versionIncompatible,
  ErrorKind.releaseTooNew,
  ErrorKind.fileLocked,
  ErrorKind.network,
  ErrorKind.wrongEnvironment,
  ErrorKind.peerWarning,
  ErrorKind.buildBlocked,
  ErrorKind.unknown,
]);

export function isKnownPluginErrorCode(code) {
  return PLUGIN_CODES.has(code);
}

export function isApiError(error) {
  return !!(error && typeof error === "object" && typeof error.code === "string");
}

export function toApiError(error, fallback = {}) {
  const fallbackCode = fallback.code || ErrorKind.commandFailed;
  if (isApiError(error)) {
    return {
      code: KNOWN_CODES.has(error.code) ? error.code : fallbackCode,
      message: error.message || fallback.message || String(error),
      detail: error.detail || fallback.detail || "",
      raw: error.raw ?? fallback.raw ?? error.message ?? String(error),
      log_path: error.log_path ?? fallback.log_path ?? null,
    };
  }

  const raw = typeof error === "string"
    ? error
    : (error?.raw ?? error?.message ?? String(error ?? ""));
  const msg = String(raw || fallback.message || fallback.detail || "操作失败");

  // GAP-014：设置默认版本时遇到非本管理器安装的 dsh 命令。
  if (msg.startsWith("DSH_CONFLICT:")) {
    return {
      code: ErrorKind.conflict,
      message: fallback.message || "默认版本被外部 dsh 命令占用",
      detail: msg.slice("DSH_CONFLICT:".length),
      raw,
    };
  }

  return {
    code: fallbackCode,
    message: fallback.message || msg,
    detail: fallback.detail || "",
    raw,
  };
}

/** 供 UI 展示：优先使用后端 message + detail，并保证原始输出仍可单独复制。 */
export function errorDisplayText(error) {
  const e = toApiError(error);
  const parts = [e.message];
  if (e.detail && e.detail !== e.message) parts.push(e.detail);
  return parts.filter(Boolean).join("\n");
}
