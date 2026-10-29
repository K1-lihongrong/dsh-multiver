/**
 * dsh-multiver-bridge —— 纯函数模块。
 *
 * 这里只放不依赖 cordis ctx、不发网络请求、不 spawn 进程的函数：
 * 输入确定 → 输出确定，可以脱离 dsh 环境直接单测。
 *
 * 从 index.js 抽出，目的是给插件侧建立回归防线。这些逻辑出过错：
 * - resolveDataRoot：root_dir 为 null 曾被误判为「未部署」
 * - parseVersionFromPath / parseIsolatedHome：版本探测的正则
 */

import { existsSync, readFileSync } from 'node:fs'
import { dirname, join, sep } from 'node:path'

/**
 * 解析 multiver 的数据根，语义与 Rust 侧 `Config::resolve_root` 一致：
 *   config.json 的 root_dir 非空 → 用它；否则 → exe 所在目录。
 *
 * 为什么必须与 Rust 一致：插件写暂存文件、multiver 读暂存文件的路径必须相同，
 * 否则用户设过 root_dir 后更新会静默失效。
 *
 * @param {string} exePath multiver exe 的绝对路径
 * @returns {{ root: string, source: 'config'|'exe-dir', configStatus: 'ok'|'missing'|'invalid' }}
 */
export function resolveDataRoot(exePath) {
  const exeDir = dirname(exePath)
  const cfgPath = join(exeDir, 'config.json')
  if (!existsSync(cfgPath)) {
    return { root: exeDir, source: 'exe-dir', configStatus: 'missing' }
  }
  try {
    const raw = JSON.parse(readFileSync(cfgPath, 'utf8'))
    const v = raw?.root_dir
    if (typeof v === 'string' && v.trim()) {
      return { root: v.trim(), source: 'config', configStatus: 'ok' }
    }
    return { root: exeDir, source: 'exe-dir', configStatus: 'ok' }
  } catch {
    return { root: exeDir, source: 'exe-dir', configStatus: 'invalid' }
  }
}

/**
 * 校验工作目录。返回 null 表示通过，否则返回 fail 结构。
 *
 * 规则：非空、绝对路径、不含 ..
 */
export function validateWorkdir(dir) {
  if (typeof dir !== 'string' || !dir.trim()) {
    return { ok: false, error: { code: 'invalid-dir', message: '工作目录不能为空', details: {} } }
  }
  const d = dir.trim()
  // 绝对路径：Windows 盘符 或 Unix 前导 /
  const isAbs = /^[A-Za-z]:[\\/]/.test(d) || d.startsWith('/') || d.startsWith('\\\\')
  if (!isAbs) {
    return { ok: false, error: { code: 'invalid-dir', message: '请提供绝对路径', details: {} } }
  }
  if (d.split(/[\\/]/).includes('..')) {
    return { ok: false, error: { code: 'invalid-dir', message: '路径不能包含 ..', details: {} } }
  }
  return null
}

/**
 * 从 dsh 入口路径里提取版本号。
 * 例：`.../versions/0.2.1-rc.2/node_modules/.bin/dsh` → `0.2.1-rc.2`
 */
export function parseVersionFromPath(p) {
  const m = /[\\/]versions[\\/]([^\\/]+)[\\/]/.exec(p)
  return m ? m[1] : null
}

/**
 * 从隔离版本 home 路径里提取版本号。
 * 例：`.../versions/0.2.1-rc.3/home/` → `0.2.1-rc.3`
 */
export function parseIsolatedHome(home) {
  const m = /[\\/]versions[\\/]([^\\/]+)[\\/]home[\\/]?$/.exec(home)
  return m ? m[1] : null
}

/** 路径规范化（统一分隔符 + 小写），用于一致性比对。 */
export function normalizePath(p) {
  return String(p).replace(/[\\/]+/g, sep).toLowerCase()
}

/**
 * 组装兼容性信息。
 *
 * @param {string[]} verifiedVersions 包内声明的已验证版本
 * @param {string|null} runningVersion 当前运行的 dsh 版本（未知时传 null）
 */
export function compatOf(verifiedVersions, runningVersion) {
  return {
    runningVersion: runningVersion ?? null,
    verifiedVersions,
    verified: runningVersion == null ? null : verifiedVersions.includes(runningVersion),
  }
}

/**
 * 下载 URL 是否允许。
 *
 * @param {string} url
 * @param {Set<string>} allowedHosts
 * @returns {{ ok: true } | { ok: false, code: string, message: string }}
 */
export function checkDownloadUrl(url, allowedHosts) {
  let u
  try {
    u = new URL(url)
  } catch {
    return { ok: false, code: 'bad-url', message: String(url) }
  }
  if (u.protocol !== 'https:') {
    return { ok: false, code: 'insecure-url', message: '仅允许 https' }
  }
  if (!allowedHosts.has(u.hostname.toLowerCase())) {
    return { ok: false, code: 'host-not-allowed', message: `不允许的下载域名：${u.hostname}` }
  }
  return { ok: true }
}
