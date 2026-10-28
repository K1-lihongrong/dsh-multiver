/**
 * dsh-multiver-bridge —— Host 半（契约 v1.1）。
 *
 * 职责：读 multiver 状态（list/which）、切默认版本（set-default）、探测 exe、打开目录。
 * 单一真相源是 multiver 的 CLI 与 config.json；本插件不缓存、不直接改 config.json。
 *
 * 传输层：ctx.webServer.register + ctx.connection.requestRejection。
 * 不用 ctx.connection.rpc.handle —— 它内部 shadow ctx 从 connection 服务自身 fiber 起查，
 * 第三方插件无论怎么 inject 都会抛 cannot get property "webServer" without inject。
 *
 * 处理顺序（固定，勿调换）：鉴权 → 方法 → 路径形状 → content-type → body → 分发。
 * 协议层错误用 HTTP 状态码（401/403/404/405/413/415/400）；
 * 业务错误一律 200 + {ok:false,error}，不 throw。
 *
 * 偏差 D6：用 node:child_process.execFile 而非 ctx.subprocess。
 * 理由：一次性只读调用、需要 exit code、无 shell、windowsHide、明确的超时。
 */

import { execFile, spawn } from 'node:child_process'
import { existsSync, realpathSync } from 'node:fs'
import { join, dirname, delimiter, sep } from 'node:path'

const MAX_BODY = 16 * 1024
const CHANNEL = '/multiver-bridge'
const VERSION_RE = /^[A-Za-z0-9][A-Za-z0-9._+-]*$/
const OPEN_TARGETS = new Set(['version-dir', 'multiver-dir', 'launch-multiver'])

export const inject = ['connection', 'webServer']

export function apply(ctx, config) {
  const cfg = {
    exe: '',
    commandTimeoutMs: 15000,
    refreshIntervalMs: 60000,
    ...config,
  }

  const route = {
    kind: 'prefix',
    path: CHANNEL,
    handler: async (req, res) => {
      try {
        // 1) 鉴权：传原始 req
        const rejection = ctx.connection.requestRejection(req)
        if (rejection !== undefined) {
          res.writeHead(rejection)
          res.end(rejection === 401 ? 'unauthorized' : 'forbidden')
          return
        }

        // 2) 方法
        if (req.method !== 'POST') {
          send(res, 405, fail('method-not-allowed', 'POST required'))
          return
        }

        // 3) 路径形状：非法形状 → 404（协议层）
        const url = new URL(req.url, 'http://localhost')
        const prefix = CHANNEL + '/'
        if (!url.pathname.startsWith(prefix)) {
          send(res, 404, fail('not-found', 'path must be ' + prefix + '<endpoint>'))
          return
        }
        const endpoint = url.pathname.slice(prefix.length)
        if (!endpoint || endpoint.includes('/')) {
          send(res, 404, fail('not-found', 'invalid endpoint shape'))
          return
        }

        // 4) content-type
        const media = String(req.headers['content-type'] ?? '').split(';')[0].trim().toLowerCase()
        if (media !== 'application/json') {
          send(res, 415, fail('unsupported-media-type', 'content-type must be application/json'))
          return
        }

        // 5) 有上限地读 body
        let size = 0
        const chunks = []
        for await (const chunk of req) {
          size += chunk.length
          if (size > MAX_BODY) {
            send(res, 413, fail('payload-too-large', 'body exceeds 16 KiB'))
            return
          }
          chunks.push(chunk)
        }
        let payload = null
        try {
          payload = size ? JSON.parse(Buffer.concat(chunks).toString('utf8')) : null
        } catch {
          send(res, 400, fail('bad-json', 'body is not JSON'))
          return
        }

        // 6) 分发：未知但形状合法的 endpoint 走业务错误（200）
        let result
        try {
          if (endpoint === 'state') result = await handleState(cfg)
          else if (endpoint === 'setDefault') result = await handleSetDefault(cfg, payload)
          else if (endpoint === 'probe') result = await handleProbe(cfg)
          else if (endpoint === 'open') result = await handleOpen(cfg, payload)
          else result = fail('unknown-endpoint', endpoint)
        } catch (e) {
          result = fail('internal', String(e?.message ?? e))
        }
        send(res, 200, result)
      } catch (e) {
        // 兜底：任何未预期异常也要有响应，不能悬挂连接
        try {
          send(res, 500, fail('internal', String(e?.message ?? e)))
        } catch {}
      }
    },
  }

  ctx.effect(() => ctx.webServer.register(route), 'multiver-bridge: routes')

  // ---------- endpoint 实现 ----------

  async function handleState(cfg) {
    const exeInfo = resolveExe(cfg.exe)
    if (!exeInfo.path) {
      return fail('exe-not-found', 'dsh-multiver.exe not found', { tried: exeInfo.tried })
    }

    const list = await runJson(exeInfo.path, ['list', '--json'], cfg.commandTimeoutMs)
    if (!list.ok) return list

    const versions = Array.isArray(list.value) ? list.value : []
    const which = await runJson(exeInfo.path, ['which', '--json'], cfg.commandTimeoutMs)

    const defaultVersion = versions.find((v) => v.is_default)?.version ?? null
    const whichVersion = which.ok ? (which.value?.version ?? null) : null
    const defaultEntry = which.ok ? (which.value?.path ?? null) : null

    const running = detectRunningVersion()
    const consistency = computeConsistency({
      exeInfo,
      versions,
      defaultVersion,
      whichOk: which.ok,
      whichVersion,
    })

    const pendingRestart = running.confidence !== 'unknown'
      && running.confidence !== 'conflict'
      && defaultVersion !== null
      && running.version !== null
      && defaultVersion !== running.version

    return {
      ok: true,
      value: {
        exe: exeInfo.path,
        detectedBy: exeInfo.detectedBy,
        mode: process.env.DSH_MULTIVER_MODE ?? null,
        defaultVersion,
        defaultEntry,
        runningVersion: running.version,
        runningDetectedBy: running.detectedBy,
        runningSources: running.sources,
        confidence: running.confidence,
        consistency,
        pendingRestart,
        versions: versions.map((v) => ({
          version: v.version,
          path: v.path,
          isDefault: !!v.is_default,
          isolated: !!v.isolated,
          installedAt: v.installed_at ?? null,
          sharedHome: !!v.shared_home,
        })),
        fetchedAt: new Date().toISOString(),
      },
    }
  }

  async function handleSetDefault(cfg, payload) {
    const version = payload?.version
    if (typeof version !== 'string' || !VERSION_RE.test(version)) {
      return fail('invalid-version', 'version must be a non-empty plain string')
    }

    const exeInfo = resolveExe(cfg.exe)
    if (!exeInfo.path) {
      return fail('exe-not-found', 'dsh-multiver.exe not found', { tried: exeInfo.tried })
    }

    // 双重校验：版本号必须来自本次 list 结果集
    const list = await runJson(exeInfo.path, ['list', '--json'], cfg.commandTimeoutMs)
    if (!list.ok) return list
    if (!Array.isArray(list.value) || !list.value.some((v) => v.version === version)) {
      return fail('version-not-installed', `版本 ${version} 未安装`)
    }

    const r = await runJson(exeInfo.path, ['set-default', version, '--json'], cfg.commandTimeoutMs)
    if (!r.ok) return r

    return {
      ok: true,
      value: {
        message: r.value?.message ?? `默认版本已设为 ${version}`,
        defaultVersion: version,
      },
    }
  }

  async function handleProbe(cfg) {
    const exeInfo = resolveExe(cfg.exe)
    if (!exeInfo.path) {
      return fail('exe-not-found', 'dsh-multiver.exe not found', { tried: exeInfo.tried })
    }

    const v = await runJson(exeInfo.path, ['version', '--json'], cfg.commandTimeoutMs)
    const l = await runJson(exeInfo.path, ['list', '--json'], cfg.commandTimeoutMs)
    return {
      ok: true,
      value: {
        exe: exeInfo.path,
        detectedBy: exeInfo.detectedBy,
        cliVersion: v.ok ? (v.value?.version ?? null) : null,
        canList: l.ok,
      },
    }
  }

  async function handleOpen(cfg, payload) {
    const target = payload?.target
    if (!OPEN_TARGETS.has(target)) {
      return fail('invalid-target', `target must be one of: ${[...OPEN_TARGETS].join(', ')}`)
    }

    const exeInfo = resolveExe(cfg.exe)
    if (!exeInfo.path) {
      return fail('exe-not-found', 'dsh-multiver.exe not found', { tried: exeInfo.tried })
    }

    if (target === 'multiver-dir') {
      return openDir(dirname(exeInfo.path))
    }

    // launch-multiver：启动 multiver 主界面（console 模式下的「回到 multiver」入口）。
    // 用 detached + unref，不阻塞插件请求；windowsHide 为 false，主窗口要可见。
    if (target === 'launch-multiver') {
      return launchMultiver(exeInfo.path)
    }

    // version-dir：服务端用新鲜 list 重算路径，不接受客户端传路径
    const version = payload?.version
    if (typeof version !== 'string' || !VERSION_RE.test(version)) {
      return fail('invalid-version', 'version must be a non-empty plain string')
    }

    const list = await runJson(exeInfo.path, ['list', '--json'], cfg.commandTimeoutMs)
    if (!list.ok) return list
    const versions = Array.isArray(list.value) ? list.value : []
    const entry = versions.find((v) => v.version === version)
    if (!entry || !entry.path) {
      return fail('version-not-installed', `版本 ${version} 未安装`)
    }

    const dataRoot = versions[0]?.path ? dirname(dirname(versions[0].path)) : null
    if (!dataRoot || !isInsideRoot(entry.path, dataRoot)) {
      return fail('outside-root', 'resolved path is outside the data root')
    }

    return openDir(entry.path)
  }

  // ---------- CLI 调用 ----------

  function runJson(exe, args, timeoutMs) {
    return new Promise((resolve) => {
      try {
        execFile(exe, args, {
          timeout: timeoutMs,
          encoding: 'utf8',
          windowsHide: true,
          maxBuffer: 8 * 1024 * 1024,
        }, (err, stdout, stderr) => {
          if (err) {
            const killed = !!err.killed
            const code = killed ? 'timeout' : 'spawn-failed'
            const message = killed
              ? `dsh-multiver 命令超时（${timeoutMs}ms）`
              : String(err.message ?? err)
            resolve(fail(code, message, { stderr: String(stderr ?? '').slice(0, 2000) }))
            return
          }
          try {
            resolve({ ok: true, value: JSON.parse(stdout) })
          } catch {
            resolve(fail('json-invalid', 'stdout is not JSON', { stdout: String(stdout).slice(0, 2000) }))
          }
        })
      } catch (e) {
        // execFile 对非法可执行文件可能在同步阶段抛错（如 EFTYPE），需同样归入 spawn-failed
        resolve(fail('spawn-failed', String(e?.message ?? e), { cause: String(e?.code ?? '') }))
      }
    })
  }

  // ---------- 打开目录 ----------

  function openDir(dirPath) {
    return new Promise((resolve) => {
      const platform = process.platform
      const cmd = platform === 'win32' ? 'explorer.exe'
        : platform === 'darwin' ? 'open'
        : 'xdg-open'

      execFile(cmd, [dirPath], { windowsHide: true, timeout: 5000 }, (err) => {
        if (err) {
          // explorer.exe 把打开目录委托给已运行实例后返回 exit code 1，属正常
          if (platform === 'win32' && typeof err.code === 'number' && err.code === 1) {
            resolve({ ok: true, value: { opened: dirPath } })
            return
          }
          resolve(fail('open-failed', String(err.message ?? err), { code: String(err.code ?? '') }))
          return
        }
        resolve({ ok: true, value: { opened: dirPath } })
      })
    })
  }

  /// 启动 multiver 主界面。成功只表示「进程已拉起」，不等待窗口出现。
  ///
  /// 失败映射：spawn 报 ENOENT 等错误 → launch-failed。
  function launchMultiver(exePath) {
    return new Promise((resolve) => {
      let settled = false
      const done = (v) => { if (!settled) { settled = true; resolve(v) } }

      try {
        const child = spawn(exePath, [], {
          detached: true,
          stdio: 'ignore',
          windowsHide: false,
        })
        child.once('error', (err) => {
          done(fail('launch-failed', String(err.message ?? err), { code: String(err.code ?? '') }))
        })
        child.once('spawn', () => {
          child.unref()
          done({ ok: true, value: { launched: true } })
        })
      } catch (e) {
        done(fail('launch-failed', String(e?.message ?? e)))
      }
    })
  }


  function isInsideRoot(targetPath, rootPath) {
    try {
      const real = realpathSync(targetPath)
      const root = realpathSync(rootPath)
      return real === root || real.startsWith(root + sep)
    } catch {
      return false
    }
  }

  // ---------- exe 探测（§5.6 顺序）----------

  function resolveExe(configured) {
    const tried = []
    const candidates = []

    const explicit = configured || process.env.DSH_MULTIVER_EXE || null
    const explicitBy = configured ? 'config' : (process.env.DSH_MULTIVER_EXE ? 'env' : null)

    if (configured) candidates.push({ path: configured, by: 'config' })
    if (process.env.DSH_MULTIVER_EXE) {
      candidates.push({ path: process.env.DSH_MULTIVER_EXE, by: 'env' })
    }

    const home = process.env.DSH_HOME
    if (home) {
      candidates.push({ path: join(dirname(home), 'dsh-multiver.exe'), by: 'dsh-home' })
      candidates.push({ path: join(dirname(dirname(home)), 'dsh-multiver.exe'), by: 'dsh-home' })
    }

    for (const dir of String(process.env.PATH ?? '').split(delimiter)) {
      if (dir) candidates.push({ path: join(dir, 'dsh-multiver.exe'), by: 'path' })
    }

    for (const c of candidates) {
      tried.push(c.path)
      try {
        if (existsSync(c.path)) {
          return { path: c.path, detectedBy: c.by, tried, explicit, explicitBy }
        }
      } catch {}
    }
    return { path: null, detectedBy: null, tried, explicit, explicitBy }
  }

  // ---------- 运行版本探测（§5.9 双头）----------

  function detectRunningVersion() {
    const sources = {}

    // A 头：multiver 注入（M3b 落地后才有值）
    const envVersion = process.env.DSH_MULTIVER_VERSION ?? null
    if (envVersion) sources.env = envVersion

    // B 头：进程自检
    const fromArgv = parseVersionFromPath(process.argv[1] ?? '')
    if (fromArgv) sources.argv = fromArgv
    const fromHome = parseIsolatedHome(process.env.DSH_HOME ?? '')
    if (fromHome) sources.dshHome = fromHome

    const selfVersion = sources.argv ?? sources.dshHome ?? null
    const selfBy = sources.argv ? 'argv' : (sources.dshHome ? 'dsh-home' : null)

    if (envVersion && selfVersion) {
      if (envVersion === selfVersion) {
        return { version: selfVersion, detectedBy: selfBy, confidence: 'high', sources }
      }
      // 冲突：B 头（进程实际加载）更真实，A 头值保留在 sources.env 供 UI 展示
      return { version: selfVersion, detectedBy: selfBy, confidence: 'conflict', sources }
    }
    if (envVersion) {
      return { version: envVersion, detectedBy: 'env', confidence: 'medium', sources }
    }
    if (selfVersion) {
      return { version: selfVersion, detectedBy: selfBy, confidence: 'low', sources }
    }
    return { version: null, detectedBy: 'unknown', confidence: 'unknown', sources }
  }

  function parseVersionFromPath(p) {
    const m = /[\\/]versions[\\/]([^\\/]+)[\\/]/.exec(p)
    return m ? m[1] : null
  }

  function parseIsolatedHome(home) {
    const m = /[\\/]versions[\\/]([^\\/]+)[\\/]home[\\/]?$/.exec(home)
    return m ? m[1] : null
  }

  // ---------- 一致性校对（H1–H3）----------

  function normalizePath(p) {
    return String(p).replace(/[\\/]+/g, sep).toLowerCase()
  }

  function computeConsistency({ exeInfo, versions, defaultVersion, whichOk, whichVersion }) {
    // defaultEntry：三值
    let defaultEntry
    if (defaultVersion === null && !whichOk) {
      // 无默认版本且 which 也报未设置 → 一致（不是 mismatch）
      defaultEntry = 'ok'
    } else if (defaultVersion === null || !whichOk) {
      defaultEntry = 'unknown'
    } else if (whichVersion === defaultVersion) {
      defaultEntry = 'ok'
    } else {
      defaultEntry = 'mismatch'
    }

    // root：数据根 vs exe 目录
    let root
    if (versions.length === 0 || !versions[0]?.path) {
      root = 'unknown'
    } else {
      const dataRoot = dirname(dirname(versions[0].path))
      root = (normalizePath(dataRoot) === normalizePath(dirname(exeInfo.path))) ? 'ok' : 'mismatch'
    }

    // exe：显式配置 vs 实际使用
    let exe
    if (!exeInfo.path) {
      exe = 'unknown'
    } else if (exeInfo.explicitBy) {
      exe = (exeInfo.detectedBy === exeInfo.explicitBy) ? 'ok' : 'mismatch'
    } else {
      exe = 'ok' // 纯自动探测
    }

    return { defaultEntry, root, exe }
  }

  // ---------- 工具 ----------

  function send(res, status, body) {
    res.writeHead(status, {
      'content-type': 'application/json; charset=utf-8',
      'cache-control': 'no-store',
    })
    res.end(JSON.stringify(body))
  }

  function fail(code, message, details = {}) {
    return { ok: false, error: { code, message, details } }
  }
}
