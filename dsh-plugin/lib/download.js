/**
 * dsh-multiver-bridge —— 下载模块。
 *
 * 承担三件事：
 * 1. 镜像回退：按 chain 顺序尝试多个源，任一成功即返回（设计见 docs/交互设计约定.md §2.8）
 * 2. 进度上报：逐块累加并通过 onProgress 回调，调用方自行决定如何暴露给 UI
 * 3. 取消：接受 AbortSignal，abort 时销毁请求并清理临时文件
 *
 * 失败判定不只看状态码——公共反代常返回 200 + HTML 错误页，
 * 因此每个源下完后还要用 checkPayloadShape 校验响应形状。
 */

import https from 'node:https'
import { createWriteStream, existsSync, unlinkSync, statSync, openSync, readSync, closeSync } from 'node:fs'
import { checkDownloadUrl, checkPayloadShape } from './pure.js'

/// 重定向跟随上限
const MAX_REDIRECTS = 5
/// 进度回调的字节阈值：每累计这么多字节报一次，避免高频回调
const PROGRESS_STEP_BYTES = 256 * 1024

function fail(code, message, details) {
  return { ok: false, error: { code, message, details: details || {} } }
}

function cleanupOne(p) {
  try { if (p && existsSync(p)) unlinkSync(p) } catch {}
}

/**
 * 读文件前 N 字节，用于形状校验。
 */
function readHead(p, n) {
  let fd
  try {
    fd = openSync(p, 'r')
    const buf = Buffer.alloc(n)
    const read = readSync(fd, buf, 0, n, 0)
    return buf.subarray(0, read)
  } catch {
    return Buffer.alloc(0)
  } finally {
    if (fd !== undefined) { try { closeSync(fd) } catch {} }
  }
}

/**
 * 单源下载：跟随重定向 → 写文件 → 返回 { bytes, finalUrl }。
 * 不负责镜像切换与形状校验。
 */
function downloadOne(url, dest, opts) {
  const { allowedHosts, timeoutMs, onProgress, signal } = opts
  return new Promise((resolve) => {
    let settled = false
    const done = (r) => { if (settled) return; settled = true; resolve(r) }

    if (signal?.aborted) {
      return done(fail('aborted', '已取消'))
    }

    const v = checkDownloadUrl(url, allowedHosts)
    if (!v.ok) return done(fail(v.code, v.message, { url }))

    const onAbort = () => {
      try { req.destroy() } catch {}
      cleanupOne(dest)
      done(fail('aborted', '已取消'))
    }
    signal?.addEventListener('abort', onAbort, { once: true })

    const req = https.get(url, { timeout: timeoutMs }, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume()
        signal?.removeEventListener('abort', onAbort)
        let next
        try { next = new URL(res.headers.location, url).toString() } catch { next = res.headers.location }
        const left = (opts._redirectsLeft ?? MAX_REDIRECTS) - 1
        if (left <= 0) return done(fail('too-many-redirects', '重定向次数过多'))
        return downloadOne(next, dest, { ...opts, _redirectsLeft: left }).then(done)
      }
      if (res.statusCode !== 200) {
        res.resume()
        signal?.removeEventListener('abort', onAbort)
        return done(fail('http-error', `HTTP ${res.statusCode}`, { url }))
      }

      const total = Number(res.headers['content-length']) || null
      let received = 0
      let lastReported = 0

      const ws = createWriteStream(dest)
      res.on('data', (chunk) => {
        received += chunk.length
        if (onProgress && received - lastReported >= PROGRESS_STEP_BYTES) {
          lastReported = received
          onProgress({ received, total })
        }
      })
      res.pipe(ws)

      ws.on('finish', () => {
        signal?.removeEventListener('abort', onAbort)
        if (onProgress) onProgress({ received, total, done: true })
        ws.close(() => done({ ok: true, value: { bytes: statSync(dest).size, finalUrl: url } }))
      })
      ws.on('error', (e) => {
        signal?.removeEventListener('abort', onAbort)
        cleanupOne(dest)
        done(fail('write-failed', String(e?.message ?? e)))
      })
    })

    req.on('timeout', () => {
      req.destroy()
      cleanupOne(dest)
      done(fail('timeout', `下载超时（${timeoutMs}ms）`))
    })
    req.on('error', (e) => {
      cleanupOne(dest)
      done(fail('network', String(e?.message ?? e)))
    })
  })
}

/**
 * 带镜像回退的下载。
 *
 * @param {string} originalUrl 原始 URL（直连源使用它；镜像源通过 rewrite 变换）
 * @param {string} dest 目标文件路径
 * @param {object} opts
 * @param {Array<{name: string, rewrite?: (u: string) => string, timeoutMs?: number}>} opts.chain 源顺序
 * @param {Set<string>} opts.allowedHosts 允许的域名
 * @param {'exe'|'json'|'text'} opts.kind 响应形状校验类型
 * @param {(p: {received: number, total: number|null, done?: boolean}) => void} [opts.onProgress]
 * @param {AbortSignal} [opts.signal]
 * @param {string|null} [opts.preferred] 上次成功的源名
 * @returns {Promise<{ok: true, value: {bytes: number, source: string, finalUrl: string}} | {ok: false, error: {code: string, message: string, details: object}}>}
 */
export async function downloadWithFallback(originalUrl, dest, opts) {
  const chain = opts.chain?.length ? opts.chain : [{ name: 'direct' }]
  const errors = []
  // preferred 优先：把它挪到队首，其余保持原序
  const ordered = (() => {
    const pref = opts.preferred
    if (!pref) return chain.slice()
    const i = chain.findIndex((s) => s.name === pref)
    if (i <= 0) return chain.slice()
    const out = chain.slice()
    const [hit] = out.splice(i, 1)
    out.unshift(hit)
    return out
  })()

  for (const src of ordered) {
    if (opts.signal?.aborted) return fail('aborted', '已取消')
    const url = src.rewrite ? src.rewrite(originalUrl) : originalUrl
    const r = await downloadOne(url, dest, {
      allowedHosts: opts.allowedHosts,
      timeoutMs: src.timeoutMs ?? 20000,
      onProgress: opts.onProgress,
      signal: opts.signal,
    })
    if (r.ok) {
      // 形状校验：公共反代常返回 200 + 错误页
      if (opts.kind) {
        const head = readHead(dest, 512)
        const shape = checkPayloadShape(head, opts.kind)
        if (!shape.ok) {
          cleanupOne(dest)
          errors.push({ source: src.name, code: shape.code, message: shape.message })
          continue
        }
      }
      return { ok: true, value: { ...r.value, source: src.name } }
    }
    errors.push({ source: src.name, ...r.error })
    // 用户主动取消时不再尝试其他源
    if (r.error.code === 'aborted') return r
  }

  return fail('all-sources-failed', '所有下载源均失败', { attempts: errors })
}

/**
 * 默认的 exe 下载源链：直连优先，失败切 gh-proxy。
 * 依据 2026-10-30 实测（见 docs/交互设计约定.md §2.8）。
 */
export function exeChain() {
  return [
    { name: 'direct', timeoutMs: 20000 },
    { name: 'gh-proxy', rewrite: (u) => `https://gh-proxy.com/${u}`, timeoutMs: 20000 },
  ]
}

/**
 * 推荐列表的源链：直连超时短（5 秒），因为它是非关键路径，拉不到就用内置副本。
 */
export function catalogChain() {
  return [
    { name: 'direct', timeoutMs: 5000 },
    { name: 'gh-proxy', rewrite: (u) => `https://gh-proxy.com/${u}`, timeoutMs: 15000 },
    { name: 'jsdelivr', rewrite: jsdelivrRewrite, timeoutMs: 15000 },
  ]
}

/**
 * 把 GitHub 地址映射为 jsDelivr 地址。
 * 仅支持 raw 形式：https://raw.githubusercontent.com/<owner>/<repo>/<ref>/<path>
 * → https://cdn.jsdelivr.net/gh/<owner>/<repo>@<ref>/<path>
 * 非该形式时返回原 URL（该源会因域名不在白名单而失败，等效跳过）。
 */
function jsdelivrRewrite(u) {
  const m = /^https:\/\/raw\.githubusercontent\.com\/([^/]+)\/([^/]+)\/([^/]+)\/(.+)$/.exec(u)
  if (!m) return u
  return `https://cdn.jsdelivr.net/gh/${m[1]}/${m[2]}@${m[3]}/${m[4]}`
}

export function cleanupFile(p) {
  cleanupOne(p)
}
