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
  const instances = opts.instances || [];
  const versionTargets = opts.versionTargets || {};
  const runningTargets = opts.runningTargets || [];
  // 录屏五语：默认 zh-CN；REC_LANG 由 scripts/record-manual.ps1 注入。
  const lang = opts.lang || process.env.REC_LANG || "zh-CN";

  // extra 序列化：JSON 值原样，字符串（函数体）标记，页面内 eval
  const extraSerialized = JSON.stringify(extra);
  const extraFnKeys = Object.entries(extra)
    .filter(([, v]) => typeof v === "function" || (typeof v === "string" && v.trim().startsWith("(")))
    .map(([k, v]) => ({ key: k, src: typeof v === "function" ? v.toString() : v }));

  await page.addInitScript(({ state, installed, launchMode, extraSerialized, extraFnKeys, instances, versionTargets, runningTargets, lang }) => {
    try { localStorage.setItem("dsh-multiver.lang", lang); } catch (_) {}
    // 录屏 spec（实例面板）需要可变的实例表；普通 E2E 不修改它。
    window.__mockInstances = instances;
    window.__mockVersionTargets = versionTargets;
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
        { name: "磁盘空间", name_key: "env.disk.name", ok: true, detail: "", detail_key: "env.disk.detail_ok", detail_args: { size: "100 GB" }, critical: false, install_hint: "", hint_key: "", install_url: "" },
        { name: "npm 源连通", name_key: "env.registry.name", ok: true, detail: "", detail_key: "env.registry.detail_ok", detail_args: {}, critical: false, install_hint: "", hint_key: "", install_url: "" },
      ],
      runtime_status: () => ({ system_node_ok: true, system_pnpm_ok: true, bundled_node_ok: false, bundled_pnpm_ok: false, portable_supported: true }),
      check_update: () => ({ has_update: false }),
      log_frontend: () => null,
      is_version_open: () => false,
      is_instance_open: () => false,
      list_instances: () => ({ instances: window.__mockInstances, version_targets: window.__mockVersionTargets }),
      list_running_targets: () => runningTargets,
      set_selected_target: () => null,
      instance_create: (args) => "i1",
      instance_rename: () => null,
      instance_remove: () => null,
      attach_instance: () => null,
      detach_instance: () => null,
      scan_instance_size: () => 0,
      open_instance_dir: () => null,
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
      // 可用版本列表（远端 npm 查询结果）。录屏/截图需要真实感的版本号。
      list_remote: () => [
        "0.2.2-alpha.2",
        "0.2.1-rc.5",
        "0.2.1-rc.4",
        "0.2.1-rc.3",
        "0.2.1-rc.2",
        "0.2.1-rc.1",
        "0.2.0",
        "0.2.0-rc.1",
        "0.1.8",
        "0.1.7",
        "0.1.6",
        "0.1.5",
        "0.1.5-rc.3",
      ],
      // 占用扫描：先给一组像样的数字，避免界面全空
      scan_usage: () => ({ "0.2.1-rc.2": { total: 1483237376, shared_size: 1181116006, exclusive_size: 302121370, shared_count: 12045 } }),
      scan_version_size: () => ({ total: 1483237376, shared_size: 1181116006, exclusive_size: 302121370, shared_count: 12045 }),
    };
    // 合并 extra：JSON 值 → 静态返回；函数体字符串 → eval 成函数
    const jsonExtra = JSON.parse(extraSerialized);
    for (const [k, v] of Object.entries(jsonExtra)) callbacks[k] = () => v;
    for (const { key, src } of extraFnKeys) {
      // eslint-disable-next-line no-eval
      callbacks[key] = (0, eval)(src);
    }
    // ---- 事件通道 ----
    // Tauri 前端的 listen() 走 invoke('plugin:event|listen', {event, target, handler})，
    // handler 是 transformCallback(handler) 的产物（本 mock 里直接就是函数本身）。
    // 后端 emit 时以 { event, id, payload } 调用该函数。
    let nextEventId = 1;
    const eventHandlers = new Map(); // eventId -> { event, handler }

    const invoke = async (cmd, args) => {
      if (cmd === "plugin:event|listen") {
        const id = nextEventId++;
        eventHandlers.set(id, { event: args.event, handler: args.handler });
        return id;
      }
      if (cmd === "plugin:event|unlisten") {
        eventHandlers.delete(args.eventId);
        return null;
      }
      if (cmd === "plugin:event|emit" || cmd === "plugin:event|emit_to") {
        __mockEmit(args.event, args.payload);
        return null;
      }
      const fn = callbacks[cmd];
      return fn ? fn(args) : null;
    };

    // 测试脚本用：page.evaluate(([e, p]) => window.__mockEmit(e, p), ["install-progress", {...}])
    window.__mockEmit = (event, payload) => {
      for (const [id, h] of eventHandlers) {
        if (h.event === event) h.handler({ event, id, payload });
      }
    };

    window.__TAURI_INTERNALS__ = {
      invoke,
      transformCallback: (cb) => cb,
      metadata: { currentWindow: { label: "main" }, currentWebview: { label: "main" } },
    };
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener: async () => {} };
  }, { state, installed, launchMode, extraSerialized, extraFnKeys, instances, versionTargets, runningTargets, lang });

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

/** 构造一个实例项（字段与 Rust InstanceView 一致）。 */
export function makeInstance(id, name, opts = {}) {
  return {
    id,
    name,
    created_at: 1762300000,
    state: "ready",
    home: `D:/dsh-data/homes/${id}`,
    mounted_versions: [],
    orphan: (opts.mounted_versions || []).length === 0,
    running_on: null,
    size: null,
    ...opts,
  };
}
