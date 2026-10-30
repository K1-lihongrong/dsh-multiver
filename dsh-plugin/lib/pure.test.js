/**
 * dsh-multiver-bridge —— 纯函数单元测试。
 *
 * 运行：node --test lib/
 * 不依赖 dsh 环境、不联网、不 spawn 进程。
 */

import { test } from 'node:test'
import assert from 'node:assert/strict'
import { mkdtempSync, writeFileSync, rmSync, mkdirSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, dirname, sep } from 'node:path'
import {
  resolveDataRoot,
  validateWorkdir,
  parseVersionFromPath,
  parseIsolatedHome,
  normalizePath,
  compatOf,
  checkDownloadUrl,
  orderSources,
  checkPayloadShape,
  progressPercent,
} from './pure.js'

/** 建临时目录，测试结束自动删。 */
function withTmp(fn) {
  const dir = mkdtempSync(join(tmpdir(), 'bridge-pure-'))
  try {
    return fn(dir)
  } finally {
    rmSync(dir, { recursive: true, force: true })
  }
}

// ---------- resolveDataRoot ----------

test('resolveDataRoot: config.json 缺失时回退 exe 目录', () => {
  withTmp((dir) => {
    const exe = join(dir, 'dsh-multiver.exe')
    const r = resolveDataRoot(exe)
    assert.equal(r.root, dir)
    assert.equal(r.source, 'exe-dir')
    assert.equal(r.configStatus, 'missing')
  })
})

test('resolveDataRoot: root_dir 有值时用配置值', () => {
  withTmp((dir) => {
    const exe = join(dir, 'dsh-multiver.exe')
    const dataDir = join(dir, 'my-data')
    writeFileSync(join(dir, 'config.json'), JSON.stringify({ root_dir: dataDir }))
    const r = resolveDataRoot(exe)
    assert.equal(r.root, dataDir)
    assert.equal(r.source, 'config')
    assert.equal(r.configStatus, 'ok')
  })
})

test('resolveDataRoot: root_dir 为 null 时回退 exe 目录（合法默认）', () => {
  withTmp((dir) => {
    const exe = join(dir, 'dsh-multiver.exe')
    writeFileSync(join(dir, 'config.json'), JSON.stringify({ root_dir: null }))
    const r = resolveDataRoot(exe)
    assert.equal(r.root, dir)
    assert.equal(r.source, 'exe-dir')
    assert.equal(r.configStatus, 'ok', 'config 存在但 root_dir 为空是合法状态')
  })
})

test('resolveDataRoot: root_dir 为空字符串时回退 exe 目录', () => {
  withTmp((dir) => {
    const exe = join(dir, 'dsh-multiver.exe')
    writeFileSync(join(dir, 'config.json'), JSON.stringify({ root_dir: '   ' }))
    const r = resolveDataRoot(exe)
    assert.equal(r.root, dir)
    assert.equal(r.source, 'exe-dir')
  })
})

test('resolveDataRoot: config.json 非法 JSON 时回退并标记 invalid', () => {
  withTmp((dir) => {
    const exe = join(dir, 'dsh-multiver.exe')
    writeFileSync(join(dir, 'config.json'), 'not json{{{')
    const r = resolveDataRoot(exe)
    assert.equal(r.root, dir)
    assert.equal(r.configStatus, 'invalid')
  })
})

// ---------- validateWorkdir ----------

test('validateWorkdir: 空值被拒绝', () => {
  assert.notEqual(validateWorkdir(''), null)
  assert.notEqual(validateWorkdir('   '), null)
  assert.notEqual(validateWorkdir(null), null)
  assert.notEqual(validateWorkdir(undefined), null)
})

test('validateWorkdir: 相对路径被拒绝', () => {
  assert.notEqual(validateWorkdir('relative/path'), null)
  assert.notEqual(validateWorkdir('./here'), null)
})

test('validateWorkdir: 含 .. 被拒绝', () => {
  assert.notEqual(validateWorkdir('/a/../b'), null)
  assert.notEqual(validateWorkdir('C:\\a\\..\\b'), null)
})

test('validateWorkdir: 合法绝对路径通过', () => {
  assert.equal(validateWorkdir('/home/user/data'), null)
  assert.equal(validateWorkdir('C:\\data'), null)
  assert.equal(validateWorkdir('D:/dsh'), null)
})

test('validateWorkdir: 错误码是 invalid-dir', () => {
  const r = validateWorkdir('relative')
  assert.equal(r.error.code, 'invalid-dir')
})

// ---------- parseVersionFromPath ----------

test('parseVersionFromPath: 提取标准版本路径', () => {
  assert.equal(
    parseVersionFromPath('E:/root/versions/0.2.1-rc.3/node_modules/.bin/dsh'),
    '0.2.1-rc.3',
  )
  assert.equal(
    parseVersionFromPath('C:\\dsh\\versions\\0.1.5-rc.3\\node_modules\\dsh\\bin.js'),
    '0.1.5-rc.3',
  )
})

test('parseVersionFromPath: 无 versions 段时返回 null', () => {
  assert.equal(parseVersionFromPath('/usr/local/bin/dsh'), null)
  assert.equal(parseVersionFromPath(''), null)
})

// ---------- parseIsolatedHome ----------

test('parseIsolatedHome: 提取隔离版本 home', () => {
  assert.equal(parseIsolatedHome('E:/root/versions/0.2.1-rc.3/home'), '0.2.1-rc.3')
  assert.equal(parseIsolatedHome('E:/root/versions/0.2.1-rc.3/home/'), '0.2.1-rc.3')
})

test('parseIsolatedHome: 非隔离 home 返回 null', () => {
  assert.equal(parseIsolatedHome('E:/root/home'), null)
  assert.equal(parseIsolatedHome('E:/root/versions/0.2.1-rc.3'), null)
})

// ---------- normalizePath ----------

test('normalizePath: 统一分隔符并小写', () => {
  const a = normalizePath('E:/Root/Data')
  const b = normalizePath('E:\\Root\\Data')
  assert.equal(a, b, '不同分隔符应归一')
})

test('normalizePath: 大小写不敏感', () => {
  assert.equal(normalizePath('/A/B'), normalizePath('/a/b'))
})

// ---------- compatOf ----------

test('compatOf: 运行版本在已验证列表内 → verified=true', () => {
  const r = compatOf(['0.2.1-rc.2', '0.2.1-rc.3'], '0.2.1-rc.3')
  assert.equal(r.verified, true)
  assert.equal(r.runningVersion, '0.2.1-rc.3')
})

test('compatOf: 运行版本不在列表 → verified=false', () => {
  const r = compatOf(['0.2.1-rc.2'], '0.3.0')
  assert.equal(r.verified, false)
})

test('compatOf: 运行版本未知 → verified=null（不误报）', () => {
  const r = compatOf(['0.2.1-rc.2'], null)
  assert.equal(r.verified, null)
  assert.equal(r.runningVersion, null)
})

// ---------- checkDownloadUrl ----------

const HOSTS = new Set(['github.com', 'objects.githubusercontent.com'])

test('checkDownloadUrl: https + 白名单域名通过', () => {
  assert.equal(checkDownloadUrl('https://github.com/x/y', HOSTS).ok, true)
  assert.equal(checkDownloadUrl('https://objects.githubusercontent.com/z', HOSTS).ok, true)
})

test('checkDownloadUrl: http 被拒绝', () => {
  const r = checkDownloadUrl('http://github.com/x', HOSTS)
  assert.equal(r.ok, false)
  assert.equal(r.code, 'insecure-url')
})

test('checkDownloadUrl: 非白名单域名被拒绝', () => {
  const r = checkDownloadUrl('https://evil.example.com/x', HOSTS)
  assert.equal(r.ok, false)
  assert.equal(r.code, 'host-not-allowed')
})

test('checkDownloadUrl: 非法 URL 被拒绝', () => {
  const r = checkDownloadUrl('not a url', HOSTS)
  assert.equal(r.ok, false)
  assert.equal(r.code, 'bad-url')
})

test('checkDownloadUrl: 域名大小写不敏感', () => {
  assert.equal(checkDownloadUrl('https://GitHub.COM/x', HOSTS).ok, true)
})

// ---------- orderSources ----------

test('orderSources: 无 preferred 时返回原序副本', () => {
  const chain = [{ name: 'a' }, { name: 'b' }]
  const out = orderSources(chain, null)
  assert.deepEqual(out.map((x) => x.name), ['a', 'b'])
  assert.notStrictEqual(out, chain)
})

test('orderSources: preferred 提到队首', () => {
  const chain = [{ name: 'a' }, { name: 'b' }, { name: 'c' }]
  const out = orderSources(chain, 'b')
  assert.deepEqual(out.map((x) => x.name), ['b', 'a', 'c'])
})

test('orderSources: preferred 已在队首时不改动', () => {
  const chain = [{ name: 'a' }, { name: 'b' }]
  const out = orderSources(chain, 'a')
  assert.deepEqual(out.map((x) => x.name), ['a', 'b'])
})

test('orderSources: preferred 不存在时返回原序', () => {
  const chain = [{ name: 'a' }, { name: 'b' }]
  const out = orderSources(chain, 'zzz')
  assert.deepEqual(out.map((x) => x.name), ['a', 'b'])
})

// ---------- checkPayloadShape ----------

test('checkPayloadShape: PE 头通过', () => {
  const head = Buffer.from([0x4d, 0x5a, 0x90, 0x00])
  assert.equal(checkPayloadShape(head, 'exe').ok, true)
})

test('checkPayloadShape: ELF 头通过', () => {
  const head = Buffer.from([0x7f, 0x45, 0x4c, 0x46])
  assert.equal(checkPayloadShape(head, 'exe').ok, true)
})

test('checkPayloadShape: HTML 错误页被识破', () => {
  const head = Buffer.from('<!DOCTYPE html><html>', 'utf8')
  const r = checkPayloadShape(head, 'exe')
  assert.equal(r.ok, false)
  assert.equal(r.code, 'bad-binary')
})

test('checkPayloadShape: 空内容被拒绝', () => {
  const r = checkPayloadShape(Buffer.alloc(0), 'exe')
  assert.equal(r.ok, false)
  assert.equal(r.code, 'empty-body')
})

test('checkPayloadShape: JSON 以 { 开头通过', () => {
  assert.equal(checkPayloadShape(Buffer.from('  {"a":1}'), 'json').ok, true)
})

test('checkPayloadShape: JSON 以 [ 开头通过', () => {
  assert.equal(checkPayloadShape(Buffer.from('[]'), 'json').ok, true)
})

test('checkPayloadShape: HTML 冒充 JSON 被识破', () => {
  const r = checkPayloadShape(Buffer.from('<html>'), 'json')
  assert.equal(r.ok, false)
  assert.equal(r.code, 'bad-json')
})

test('checkPayloadShape: text 类型不校验内容', () => {
  assert.equal(checkPayloadShape(Buffer.from('anything'), 'text').ok, true)
})

// ---------- progressPercent ----------

test('progressPercent: 正常计算', () => {
  assert.equal(progressPercent(512, 1024), 50)
})

test('progressPercent: total 未知时返回 null', () => {
  assert.equal(progressPercent(512, null), null)
  assert.equal(progressPercent(512, 0), null)
  assert.equal(progressPercent(512, NaN), null)
})

test('progressPercent: 超出范围被夹紧', () => {
  assert.equal(progressPercent(2048, 1024), 100)
  assert.equal(progressPercent(-1, 1024), 0)
})

