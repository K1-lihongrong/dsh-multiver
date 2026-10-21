// v0.2.14：Playwright E2E 公共工具。
// 纯 web 环境（vite preview）无 Tauri 后端，需 mock window.__TAURI_INTERNALS__。
//
// 关键：page.addInitScript 的参数走 structured clone，函数会丢失。
// 故 extra 只接受「JSON 值」或「字符串形式的函数体」（如 "(args) => ..."）。

/**
 * 注入 Tauri mock。必须在页面加载前调用。
 * @param {import('@playwright/test').Page} page
 * @param {object} opts
 * @param {object} [opts.state]       get_state 返回值
 * @param {Array}  [opts.installed]   list_installed 返回值
 * @param {string|null} [opts.launchMode] get_launch_mode 返回值
 * @param {Record<string, any>} [opts.extra] 其它命令：值为 JSON（静态返回）或字符串（函数体，如 "(args) => Promise.resolve('ok')"）
 */
export async function mockTauri(page, opts = {}) {
  const state = opts.state || {
    version: "0.2.14",
    root_dir: "D:/dsh-data",
    default_version: null,
    isolated_versions: [],
    broken_versions: [],
    use_official_dsh_home: false,
    runtime_prompt_dismissed: false,
    maintenance: { auto: true, last_cleanup: null, last_prune: null },
  };
  const installed = opts.installed || [];
  const launchMode = opts.launchMode ?? null;
  const extra = opts.extra || {};

  // extra 序列化：JSON 值原样，字符串（函数体）标记，页面内 eval
  const extraSerialized = JSON.stringify(extra);
  const extraFnKeys = Object.entries(extra)
    .filter(([, v]) => typeof v === "function" || (typeof v === "string" && v.trim().startsWith("(")))
    .map(([k, v]) => ({ key: k, src: typeof v === "function" ? v.toString() : v }));

  await page.addInitScript(({ state, installed, launchMode, extraSerialized, extraFnKeys }) => {
    try { localStorage.setItem("dsh-multiver.lang", "zh-CN"); } catch (_) {}
    const listeners = new Map();
    const callbacks = {
      get_state: () => state,
      list_installed: () => installed,
      get_launch_mode: () => launchMode,
      check_env: () => [
        { name: "Node.js", name_key: "env.node.name", ok: true, detail: "v22.19.0", detail_key: "env.node.detail_ok", detail_args: { v: "v22.19.0" }, critical: true, install_hint: "", hint_key: "", install_url: "" },
        { name: "pnpm", name_key: "env.pnpm.name", ok: true, detail: "v11.17.0", detail_key: "", detail_args: {}, critical: true, install_hint: "", hint_key: "", install_url: "" },
        { name: "git", name_key: "env.git.name", ok: true, detail: "git version 2.47.3", detail_key: "", detail_args: {}, critical: false, install_hint: "", hint_key: "", install_url: "" },
        { name: "根目录可写", name_key: "env.writable.name", ok: true, detail: "D:/dsh-data", detail_key: "", detail_args: {}, critical: true, install_hint: "", hint_key: "", install_url: "" },
        { name: "磁盘空间", name_key: "env.disk.name", ok: true, detail: "可用 100 GB", detail_key: "", detail_args: {}, critical: false, install_hint: "", hint_key: "", install_url: "" },
        { name: "npm 源连通", name_key: "env.registry.name", ok: true, detail: "可达", detail_key: "", detail_args: {}, critical: false, install_hint: "", hint_key: "", install_url: "" },
      ],
      runtime_status: () => ({ system_node_ok: true, system_pnpm_ok: true, bundled_node_ok: false, bundled_pnpm_ok: false, portable_supported: true }),
      check_update: () => ({ has_update: false }),
      log_frontend: () => null,
      is_version_open: () => false,
      // 默认：安装前 inspect 成功（组合包）；需要失败场景时用 extra 覆盖
      ds_inspect_spec: () => ({
        kind: "npm",
        name: "test-pkg",
        version: "1.0.0",
        description: "",
        declares_bundle: true,
        problem: null,
        detail: null,
      }),
      ds_probe_registries: () => ({ kind: null, url: null, ms: null }),
    };
    // 合并 extra：JSON 值 → 静态返回；函数体字符串 → eval 成函数
    const jsonExtra = JSON.parse(extraSerialized);
    for (const [k, v] of Object.entries(jsonExtra)) callbacks[k] = () => v;
    for (const { key, src } of extraFnKeys) {
      // eslint-disable-next-line no-eval
      callbacks[key] = (0, eval)(src);
    }
    const invoke = async (cmd, args) => {
      const fn = callbacks[cmd];
      return fn ? fn(args) : null;
    };
    window.__TAURI_INTERNALS__ = {
      invoke,
      transformCallback: (cb) => cb,
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: async () => {} };
  }, { state, installed, launchMode, extraSerialized, extraFnKeys });

  // 注意：mockTauri 调用后，页面才能 goto。
}

/** 便捷：构造一个已安装版本项（字段名与 Rust VersionInfo 一致） */
export function makeVersion(version, opts = {}) {
  return {
    version,
    installed_at: "2026-10-01",
    isolated: opts.isolated ?? false,
    is_default: opts.isDefault ?? false,
    broken: false,
    note: null,
    ...opts,
  };
}
