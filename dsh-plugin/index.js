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
import {
  existsSync, realpathSync, readFileSync, writeFileSync,
  mkdirSync, unlinkSync, renameSync, statSync,
  statfsSync,
} from 'node:fs'
import { join, dirname, delimiter, sep } from 'node:path'
import { createHash } from 'node:crypto'
import {
  resolveDataRoot,
  validateWorkdir as validateWorkdirPure,
  parseVersionFromPath,
  parseIsolatedHome,
  normalizePath,
  compatOf as compatOfPure,
  progressPercent,
} from './lib/pure.js'
import { downloadWithFallback, exeChain } from './lib/download.js'

const MAX_BODY = 16 * 1024
/// 部署下载前要求的最小可用空间（100 MB，exe 本身约 10 MB，留足余量）
const MIN_FREE_BYTES = 100 * 1024 * 1024
/// 下载重定向的最大跟随次数
/// 允许下载的主机名（GitHub 发布资产会 302 到 objects.githubusercontent.com）
/// 镜像域名来自 2026-10-30 实测，见 docs/交互设计约定.md §2.8
const ALLOWED_DOWNLOAD_HOSTS = new Set([
  'github.com',
  'objects.githubusercontent.com',
  'github-releases.githubusercontent.com',
  'gh-proxy.com',
  'cdn.jsdelivr.net',
])
const CHANNEL = '/multiver-bridge'
const VERSION_RE = /^[A-Za-z0-9][A-Za-z0-9._+-]*$/
const OPEN_TARGETS = new Set(['version-dir', 'multiver-dir', 'launch-multiver', 'multiver-settings'])

// 部署引导（阶段 2）。下载源固定为本仓库的 latest Release，不接受任意 URL。
const RELEASE_REPO = 'K1-lihongrong/dsh-multiver'
const EXE_ASSET = 'dsh-multiver.exe'
const SUMS_ASSET = 'SHA256SUMS.txt'
const EXE_NAME = process.platform === 'win32' ? 'dsh-multiver.exe' : 'dsh-multiver'

// 默认工作目录：%LOCALAPPDATA%\dsh-multiver（Win）/ ~/.local/share/dsh-multiver（Unix）
const DEFAULT_WORKDIR = (() => {
  try {
    if (process.platform === 'win32') {
      const base = process.env.LOCALAPPDATA || process.env.APPDATA
      if (base) return join(base, 'dsh-multiver')
    } else {
      const home = process.env.HOME
      if (home) return join(home, '.local', 'share', 'dsh-multiver')
    }
  } catch {}
  return null
})()

// 已验证的 dsh 版本（来自本包 package.json 的 dshMultiver.verifiedDshVersions）。
// 仅用于 UI 提示，不做安装拦截——engines 字段刻意省略，安装层零约束。
const VERIFIED_DSH_VERSIONS = (() => {
  try {
    const pkg = JSON.parse(readFileSync(new URL('./package.json', import.meta.url), 'utf8'))
    const list = pkg?.dshMultiver?.verifiedDshVersions
    return Array.isArray(list) ? list.filter((v) => typeof v === 'string' && v) : []
  } catch {
    return []
  }
})()

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
          else if (endpoint === 'provision') result = await handleProvision(cfg, payload)
          else if (endpoint === 'update') result = await handleUpdate(cfg, payload)
          else if (endpoint === 'progress') result = handleDownloadProgress(payload)
          else if (endpoint === 'cancel') result = handleDownloadCancel(payload)
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

    // 部署态探测（阶段 2）：exe 缺失或 root 未设置时，返回 provisioned=false +
    // 向导所需信息，而不是直接报错——否则 Client 无法进入部署引导。
    // 真相源是 multiver 自身的 config.json，插件不另存。
    const provision = probeProvision(exeInfo)
    if (!provision.provisioned) {
      return {
        ok: true,
        value: {
          provisioned: false,
          provision,
          mode: process.env.DSH_MULTIVER_MODE ?? null,
          compatibility: compatOf(detectRunningVersion().version),
          fetchedAt: new Date().toISOString(),
        },
      }
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
        provisioned: true,
        provision,
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
        // 兼容性提示（不阻断功能）：包内声明的已验证 dsh 版本 vs 当前运行版本。
        // 未知运行版本时 verified 为 null，UI 不显示提示。
        compatibility: compatOf(running.version),
        // 已暂存待安装的更新（multiver 下次启动时由 Rust 侧完成替换）
        pendingUpdate: readPendingUpdate(exeInfo.path),
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

    // multiver-settings：打开 multiver 并滚动到「路径设置」区块。
    // 依赖 multiver v0.3.2+ 的 --open-settings 参数；旧版会忽略该参数、只打开主界面，
    // 用户仍能自己滚动找到设置，故不视为错误，但在返回值里标注 degraded 供前端提示。
    if (target === 'multiver-settings') {
      return launchMultiver(exeInfo.path, ['--open-settings=paths'])
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
  function launchMultiver(exePath, extraArgs) {
    return new Promise((resolve) => {
      let settled = false
      const done = (v) => { if (!settled) { settled = true; resolve(v) } }

      try {
        const child = spawn(exePath, extraArgs || [], {
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

  // ---------- multiver 更新管理（阶段 3）----------

  /// 更新管理统一入口。委托 multiver 自带 updater，插件不重复实现替换逻辑。
  ///   step=check   只查（等价 update --dry-run --json），不下载
  ///   step=stage   暂存：下载到 <root>/.update/，校验后写 pending.json，
  ///                由 multiver 下次启动时安装（不打断任何会话）
  ///   step=apply   立即更新（等价 update --json）：下载 + 校验 + 替换 + 重启
  ///
  /// embedded 模式下 apply 自动降级为 stage——内嵌窗口的 multiver 正在运行，
  /// 替换它的 exe 会打断当前会话，且 Windows 上无法覆盖运行中的文件。
  async function handleUpdate(cfg, payload) {
    const step = payload?.step
    if (step !== 'check' && step !== 'stage' && step !== 'apply') {
      return fail('invalid-step', 'step must be check, stage or apply')
    }
    const exeInfo = resolveExe(cfg.exe)
    if (!exeInfo.path) {
      return fail('exe-not-found', 'dsh-multiver.exe not found', { tried: exeInfo.tried })
    }

    const mode = process.env.DSH_MULTIVER_MODE ?? null
    const running = detectRunningVersion()

    if (step === 'check') {
      const args = ['update', '--dry-run', '--json']
      const r = await runJson(exeInfo.path, args, cfg.commandTimeoutMs + 30000)
      if (!r.ok) return r
      const v = r.value || {}
      return {
        ok: true,
        value: {
          current: v.current ?? null,
          latest: v.latest ?? null,
          hasUpdate: !!v.has_update,
          downloadUrl: v.download_url ?? null,
          releaseUrl: RELEASE_REPO ? `https://github.com/${RELEASE_REPO}/releases` : null,
          error: v.error ?? null,
          // 运行中实例：embedded 模式下替换 exe 会打断当前会话，
          // 故 apply 会自动降级为 stage（暂存，下次启动安装）
          runningInMultiver: mode === 'embedded',
          mode,
          runningVersion: running.version,
          // 已暂存待安装的更新（下次 multiver 启动时生效）
          pendingUpdate: readPendingUpdate(exeInfo.path),
        },
      }
    }

    // embedded 模式下不能立即替换，降级为暂存
    if (step === 'apply' && mode === 'embedded') {
      const staged = await stageUpdate(cfg, exeInfo)
      if (!staged.ok) return staged
      return {
        ok: true,
        value: {
          ...staged.value,
          deferred: true,
          message: staged.value.staged
            ? `已暂存 ${staged.value.version}，multiver 下次启动时自动安装`
            : '已是最新版本',
        },
      }
    }

    if (step === 'stage') {
      return await stageUpdate(cfg, exeInfo)
    }

    // step === 'apply'（非 embedded）

    const r = await runJson(exeInfo.path, ['update', '--json'], 600000)
    if (!r.ok) return r

    const v = r.value || {}
    // CLI 在「已是最新」时同样 exit 0，返回 {has_update:false}。
    // 若 update 成功触发替换，CLI 输出的是 ok/message 结构（无 has_update 字段）。
    if (Object.prototype.hasOwnProperty.call(v, 'has_update') && v.has_update === false) {
      return {
        ok: true,
        value: {
          handedOff: false,
          upToDate: true,
          current: v.current ?? null,
          latest: v.latest ?? null,
        },
      }
    }
    return {
      ok: true,
      value: {
        handedOff: true,
        upToDate: false,
        message: v.message ?? '更新已就绪，管理器将退出以完成替换',
      },
    }
  }

  // ---------- 暂存更新（staged update）----------

  /// 读取已暂存的更新标记（<root>/.update/pending.json）。
  /// 返回 {version, stagedAt} 或 null。失败静默——这只是展示信息。
  function readPendingUpdate(exePath) {
    try {
      // 顺手清理下载中断留下的临时文件（这里正好是每次刷新的必经路径）
      const r = resolveDataRoot(exePath)
      cleanupDownloadTemp(r.root)
      const p = join(r.root, '.update', 'pending.json')
      if (!existsSync(p)) return null
      const j = JSON.parse(readFileSync(p, 'utf8'))
      if (!j || typeof j.version !== 'string') return null
      return { version: j.version, stagedAt: j.staged_at ?? null }
    } catch {
      return null
    }
  }

  /// 暂存更新：查最新版 → 下载 → 校验 → 写到 <数据根>/.update/ → 写 pending.json。
  /// multiver 下次启动时由 Rust 侧 staged 模块完成替换（见 src-tauri/src/staged.rs）。
  /// 数据根用 resolveDataRoot 解析，保证与 multiver 的读取位置一致。
  async function stageUpdate(cfg, exeInfo) {
    const check = await runJson(exeInfo.path, ['update', '--dry-run', '--json'], cfg.commandTimeoutMs + 30000)
    if (!check.ok) return check
    const v = check.value || {}
    if (!v.has_update) {
      return { ok: true, value: { staged: false, current: v.current ?? null, latest: v.latest ?? null } }
    }
    if (!v.download_url) {
      return fail('download-url-missing', 'Release 资产缺失，无法下载新版本')
    }

    // 用与 multiver 一致的数据根解析——否则用户设过 root_dir 后暂存位置与安装位置不符
    const root = resolveDataRoot(exeInfo.path).root
    const stageDir = join(root, '.update')
    const stagedExe = join(stageDir, 'dsh-multiver.exe.new')
    const tmpExe = join(stageDir, '.download.tmp')

    try {
      mkdirSync(stageDir, { recursive: true })
    } catch (e) {
      return fail('dir-create-failed', String(e?.message ?? e))
    }

    // 磁盘空间检查：与部署下载保持一致（exe 约 10 MB，留 100 MB 余量）
    try {
      const st = statfsSync(stageDir)
      const freeBytes = Number(st.bavail) * Number(st.bsize)
      if (Number.isFinite(freeBytes) && freeBytes < MIN_FREE_BYTES) {
        return fail('disk-full', `磁盘剩余空间不足：需要至少 ${Math.round(MIN_FREE_BYTES / 1048576)} MB，当前 ${Math.round(freeBytes / 1048576)} MB`, {
          needBytes: MIN_FREE_BYTES,
          freeBytes,
        })
      }
    } catch {
      // statfs 不可用 → 跳过，交给写入失败兜底
    }

    // 同一时刻只允许一个下载任务，避免连点产生并发写同一临时文件
    const running = hasActiveDownload()
    if (running) {
      return fail('download-busy', '已有下载任务在进行中', { taskId: running.id })
    }
    const task = startDownloadTask('stage')
    let dl
    try {
      dl = await downloadWithFallback(v.download_url, tmpExe, {
        chain: exeChain(),
        allowedHosts: ALLOWED_DOWNLOAD_HOSTS,
        kind: 'exe',
        onProgress: task.onProgress,
        signal: task.signal,
      })
    } finally {
      task.finish(dl && !dl.ok ? dl.error : null)
    }
    if (!dl.ok) {
      cleanup([tmpExe])
      return dl
    }

    // 校验：优先用 CLI 给的 sha256，缺失则只做大小检查
    const expected = (v.sha256 || '').toLowerCase()
    if (expected) {
      const actual = sha256File(tmpExe)
      if (actual !== expected) {
        cleanup([tmpExe])
        return fail('checksum-mismatch', 'SHA256 校验失败', { expected, actual })
      }
    }

    try {
      renameSync(tmpExe, stagedExe)
    } catch (e) {
      cleanup([tmpExe])
      return fail('rename-failed', String(e?.message ?? e))
    }

    const pending = {
      version: String(v.latest ?? ''),
      sha256: expected,
      staged_at: Math.floor(Date.now() / 1000),
      size: statSync(stagedExe).size,
    }
    try {
      writeFileSync(join(stageDir, 'pending.json'), JSON.stringify(pending, null, 2))
    } catch (e) {
      cleanup([stagedExe])
      return fail('write-failed', String(e?.message ?? e))
    }

    return {
      ok: true,
      value: {
        staged: true,
        version: pending.version,
        size: pending.size,
        verified: !!expected,
        stageDir,
      },
    }
  }

  // ---------- 部署引导（阶段 2）----------

  /// 统一入口：按 payload.step 分发。
  ///   step=prepare  选目录 + 可选下载/校验（返回后续步骤所需信息，不落盘业务状态）
  ///   step=download 下载 exe + 校验 SHA256 + 落到工作目录
  ///   step=adopt    采用已有 exe 路径
  ///   step=setRoot  调用 multiver CLI set-root
  ///   step=launch   启动 multiver 主界面
  async function handleProvision(cfg, payload) {
    const step = payload?.step
    if (step === 'download') return provisionDownload(payload)
    if (step === 'adopt') return provisionAdopt(payload)
    if (step === 'setRoot') return provisionSetRoot(cfg, payload)
    if (step === 'launch') return provisionLaunch(cfg)
    return fail('invalid-step', 'step must be one of: download, adopt, setRoot, launch')
  }

  /// 校验工作目录：必须是绝对路径，且父目录可创建。
  // validateWorkdir 实现在 lib/pure.js，这里仅作别名保持调用点可读
  const validateWorkdir = validateWorkdirPure

  /// 下载 exe 与 SHA256SUMS.txt，校验后落到 <workdir>/dsh-multiver.exe。
  /// 失败时清理临时文件，不留半成品。
  async function provisionDownload(payload) {
    const bad = validateWorkdir(payload?.workdir)
    if (bad) return bad
    const workdir = payload.workdir.trim()
    const tag = payload?.tag ? String(payload.tag) : null

    try {
      mkdirSync(workdir, { recursive: true })
    } catch (e) {
      return fail('dir-create-failed', String(e?.message ?? e))
    }
    // 可写性探测
    const probe = join(workdir, '.multiver-bridge-write-test')
    try {
      writeFileSync(probe, 'ok')
      unlinkSync(probe)
    } catch (e) {
      return fail('dir-readonly', String(e?.message ?? e))
    }

    // 磁盘空间检查：exe 约 10 MB，留 100 MB 余量避免写满。
    // statfs 在部分平台不可用 → 拿不到就跳过检查（不阻断）。
    const need = MIN_FREE_BYTES
    try {
      const st = statfsSync(workdir)
      const freeBytes = Number(st.bavail) * Number(st.bsize)
      if (Number.isFinite(freeBytes) && freeBytes < need) {
        return fail('disk-full', `磁盘剩余空间不足：需要至少 ${Math.round(need / 1048576)} MB，当前 ${Math.round(freeBytes / 1048576)} MB`, {
          needBytes: need,
          freeBytes,
        })
      }
    } catch {
      // 平台不支持 statfs / 路径不可读 → 跳过，交给后续写入失败兜底
    }

    const base = tag
      ? `https://github.com/${RELEASE_REPO}/releases/download/${tag}`
      : `https://github.com/${RELEASE_REPO}/releases/latest/download`

    const tmpExe = join(workdir, '.dsh-multiver.exe.download')
    const tmpSum = join(workdir, '.SHA256SUMS.download')

    // 同一时刻只允许一个下载任务
    const busy = hasActiveDownload()
    if (busy) {
      return fail('download-busy', '已有下载任务在进行中', { taskId: busy.id })
    }

    try {
      const exeTask = startDownloadTask('provision-exe')
      let dlExe
      try {
        dlExe = await downloadWithFallback(`${base}/${EXE_ASSET}`, tmpExe, {
          chain: exeChain(),
          allowedHosts: ALLOWED_DOWNLOAD_HOSTS,
          kind: 'exe',
          onProgress: exeTask.onProgress,
          signal: exeTask.signal,
        })
      } finally {
        exeTask.finish(dlExe && !dlExe.ok ? dlExe.error : null)
      }
      if (!dlExe.ok) return dlExe

      // SHA256SUMS.txt 可能不存在（旧 Release）；存在则强制校验，不存在只警告
      const sumTask = startDownloadTask('provision-sums')
      let dlSum
      try {
        dlSum = await downloadWithFallback(`${base}/${SUMS_ASSET}`, tmpSum, {
          chain: exeChain(),
          allowedHosts: ALLOWED_DOWNLOAD_HOSTS,
          kind: 'text',
          onProgress: sumTask.onProgress,
          signal: sumTask.signal,
        })
      } finally {
        sumTask.finish(dlSum && !dlSum.ok ? dlSum.error : null)
      }
      let checksum = null
      if (dlSum.ok) {
        const text = readFileSync(tmpSum, 'utf8')
        const line = text.split(/\r?\n/).find((l) => l.trim().endsWith(EXE_ASSET))
        if (!line) {
          cleanup([tmpExe, tmpSum])
          return fail('checksum-missing', `${SUMS_ASSET} 中未找到 ${EXE_ASSET}`)
        }
        checksum = line.trim().split(/\s+/)[0].toLowerCase()
        const actual = sha256File(tmpExe)
        if (actual !== checksum) {
          cleanup([tmpExe, tmpSum])
          return fail('checksum-mismatch', 'SHA256 校验失败', { expected: checksum, actual })
        }
      }

      const finalExe = join(workdir, EXE_NAME)
      try {
        renameSync(tmpExe, finalExe)
      } catch (e) {
        // Windows 上目标若被占用会失败
        cleanup([tmpExe, tmpSum])
        return fail('rename-failed', String(e?.message ?? e))
      }
      cleanup([tmpSum])

      return {
        ok: true,
        value: {
          exe: finalExe,
          verified: !!checksum,
          checksum: checksum,
          size: statSync(finalExe).size,
          workdir,
          tag,
        },
      }
    } catch (e) {
      cleanup([tmpExe, tmpSum])
      return fail('download-failed', String(e?.message ?? e))
    }
  }

  /// 采用用户已有的 exe：校验存在且可执行，不复制。
  function provisionAdopt(payload) {
    const p = payload?.exe
    if (typeof p !== 'string' || !p.trim()) {
      return fail('invalid-exe', 'exe 路径不能为空')
    }
    const exe = p.trim()
    if (!existsSync(exe)) {
      return fail('exe-missing', `文件不存在：${exe}`)
    }
    try {
      const st = statSync(exe)
      if (!st.isFile()) return fail('exe-missing', '不是文件')
    } catch (e) {
      return fail('exe-missing', String(e?.message ?? e))
    }
    return { ok: true, value: { exe, adopted: true } }
  }

  /// 调用 multiver CLI set-root（唯一写配置的通道，插件不直接改 config.json）。
  async function provisionSetRoot(cfg, payload) {
    const bad = validateWorkdir(payload?.workdir)
    if (bad) return bad
    const exe = payload?.exe
    if (typeof exe !== 'string' || !exe.trim() || !existsSync(exe.trim())) {
      return fail('exe-missing', '请先获取 dsh-multiver.exe')
    }
    const r = await runJson(exe.trim(), ['set-root', payload.workdir.trim(), '--json'], cfg.commandTimeoutMs)
    if (!r.ok) return r
    return {
      ok: true,
      value: {
        root: r.value?.root ?? payload.workdir.trim(),
        previous: r.value?.previous ?? null,
      },
    }
  }

  /// 启动 multiver 主界面（部署完成后引导用户进入）。
  async function provisionLaunch(cfg) {
    const exeInfo = resolveExe(cfg.exe)
    if (!exeInfo.path) return fail('exe-not-found', 'dsh-multiver.exe not found')
    return launchMultiver(exeInfo.path)
  }

  // ---------- 下载任务注册表（进度与取消的载体）----------
  //
  // 插件侧只有 HTTP 请求-响应，没有服务端推送通道，所以进度由 Client 轮询 progress 端点取得。
  // 每个任务有唯一 id，同一时刻同一用途只允许一个任务（避免用户连点）。
  const downloadTasks = new Map()
  let downloadSeq = 0

  /// 创建下载任务，返回 { id, signal, onProgress, finish }。
  /// onProgress 更新内存里的进度，供 progress 端点读取。
  function startDownloadTask(kind) {
    const id = `dl-${++downloadSeq}`
    const controller = new AbortController()
    const task = {
      id,
      kind,
      received: 0,
      total: null,
      percent: null,
      startedAt: Date.now(),
      aborted: false,
      error: null,
      done: false,
    }
    downloadTasks.set(id, task)
    const onProgress = (p) => {
      task.received = p.received
      task.total = p.total ?? null
      task.percent = progressPercent(p.received, p.total ?? null)
    }
    const finish = (err) => {
      task.done = true
      if (err) task.error = err
      // 保留 60 秒供 Client 取最终状态，之后清掉避免无限增长
      setTimeout(() => downloadTasks.delete(id), 60000).unref?.()
    }
    return { id, signal: controller.signal, controller, onProgress, finish, task }
  }

  /// 只允许一个下载任务在跑：有未结束的任务时拒绝新建。
  function hasActiveDownload() {
    for (const t of downloadTasks.values()) {
      if (!t.done && !t.aborted) return t
    }
    return null
  }

  function cleanup(paths) {
    for (const p of paths) {
      try { if (existsSync(p)) unlinkSync(p) } catch {}
    }
  }

  /// 清理暂存目录里的下载临时文件。
  /// 下载中途进程被杀会留下 .download.tmp，启动检查时顺手清掉。
  function cleanupDownloadTemp(root) {
    try {
      const p = join(root, '.update', '.download.tmp')
      if (existsSync(p)) unlinkSync(p)
    } catch {}
  }

  function sha256File(p) {
    return createHash('sha256').update(readFileSync(p)).digest('hex')
  }

  // ---------- 下载进度与取消（GAP-016）----------

  /// 查询下载任务进度。不传 id 时返回当前活跃任务；都不存在时返回 idle。
  ///
  /// Client 在下载期间每 400ms 轮询一次（插件侧无推送通道，只能轮询）。
  function handleDownloadProgress(payload) {
    const id = payload?.id
    if (id) {
      const t = downloadTasks.get(id)
      if (!t) return fail('task-not-found', '下载任务不存在或已过期', { id })
      return { ok: true, value: publicTask(t) }
    }
    const active = hasActiveDownload()
    if (active) return { ok: true, value: publicTask(active) }
    return { ok: true, value: { idle: true } }
  }

  /// 对外暴露的任务字段（不泄露 controller 等内部引用）。
  function publicTask(t) {
    return {
      id: t.id,
      kind: t.kind,
      received: t.received,
      total: t.total,
      percent: t.percent,
      elapsedMs: Date.now() - t.startedAt,
      aborted: t.aborted,
      done: t.done,
      error: t.error,
    }
  }

  /// 取消下载。已结束或不存在时返回 cancelled:false 而非报错——前端不必区分。
  function handleDownloadCancel(payload) {
    const id = payload?.id
    const t = downloadTasks.get(id)
    if (!t) return { ok: true, value: { cancelled: false, reason: 'not-found' } }
    if (t.done) return { ok: true, value: { cancelled: false, reason: 'already-done' } }
    t.aborted = true
    return { ok: true, value: { cancelled: true, id } }
  }

  // ---------- 部署态探测（阶段 2）----------

  /// 读 multiver 自身的 config.json，判断「exe 可用 + root_dir 已设置 + 目录可写」。
  /// 插件不另存部署状态，避免与 multiver 配置分叉。
  function probeProvision(exeInfo) {
    const out = {
      provisioned: false,
      exe: exeInfo.path,
      detectedBy: exeInfo.detectedBy,
      root: null,
      rootSource: null,
      exeWritable: false,
      defaultWorkdir: DEFAULT_WORKDIR,
      releaseRepo: RELEASE_REPO,
      platform: process.platform,
      reasons: [],
    }
    if (!exeInfo.path) {
      out.reasons.push('exe-missing')
      return out
    }
    // 数据根解析：与 multiver 的 resolve_root 语义一致（见 resolveDataRoot）
    const resolved = resolveDataRoot(exeInfo.path)
    if (resolved.configStatus === 'missing') {
      out.reasons.push('config-missing')
      return out
    }
    if (resolved.configStatus === 'invalid') {
      out.reasons.push('config-invalid')
      return out
    }
    const root = resolved.root
    out.root = root
    out.rootSource = resolved.source
    if (!existsSync(root)) {
      out.reasons.push('root-missing')
      return out
    }
    // 可写性：写一个探针文件再删掉
    const probe = join(root, '.multiver-bridge-write-test')
    try {
      writeFileSync(probe, 'ok')
      unlinkSync(probe)
      out.rootWritable = true
    } catch {
      out.rootWritable = false
      out.reasons.push('root-readonly')
      return out
    }
    out.rootWritable = true
    out.provisioned = true
    return out
  }

  // compatOf 实现在 lib/pure.js，这里绑定已验证版本列表
  const compatOf = (runningVersion) => compatOfPure(VERIFIED_DSH_VERSIONS, runningVersion)

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

  // parseVersionFromPath / parseIsolatedHome / normalizePath 实现在 lib/pure.js

  // ---------- 一致性校对（H1–H3）----------

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
