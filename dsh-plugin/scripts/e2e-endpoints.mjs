#!/usr/bin/env node
/**
 * dsh-multiver-bridge —— 端点集成测试（手工执行，不进 CI）。
 *
 * 覆盖 v0.2.1 新增的下载进度与取消逻辑。这些端点依赖真实 dsh 实例，
 * CI 环境没有 dsh，所以做成可手工复跑的脚本而非自动化测试。
 *
 * 用法：
 *   node scripts/e2e-endpoints.mjs <baseUrl> <token>
 *
 * 前置：
 *   1. 起一个装了本插件的 dsh 实例（dsh web --port <p>）
 *   2. 从浏览器或 curl 拿到 dsh-auth token 与端口
 *
 * 覆盖用例：
 *   - progress 无任务时返回 idle
 *   - cancel 不存在的任务返回 cancelled:false（不报错）
 *   - 触发一次 stage 下载，轮询 progress 能看到 received 增长
 *   - 下载中途发 cancel，最终 stage 返回 aborted
 */

const [, , baseUrl, token] = process.argv
if (!baseUrl || !token) {
  console.error('用法：node scripts/e2e-endpoints.mjs <baseUrl> <token>')
  console.error('例：node scripts/e2e-endpoints.mjs http://127.0.0.1:3080 dsh-auth-xxx')
  process.exit(1)
}

const CH = '/multiver-bridge'
let passed = 0
let failed = 0

function ok(name, cond, detail) {
  if (cond) { passed++; console.log('  ✓ ' + name) }
  else { failed++; console.log('  ✗ ' + name + (detail ? ' — ' + detail : '')) }
}

async function rpc(endpoint, payload) {
  const res = await fetch(baseUrl + CH + '/' + endpoint, {
    method: 'POST',
    headers: { 'content-type': 'application/json', cookie: token },
    body: JSON.stringify(payload ?? {}),
  })
  if (!res.ok) throw new Error('HTTP ' + res.status)
  return res.json()
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

async function main() {
  console.log('=== 1. progress 无任务 ===')
  const idle = await rpc('progress', {})
  ok('返回 ok', idle.ok === true, JSON.stringify(idle))
  ok('idle 为 true 或返回活跃任务', idle.value?.idle === true || !!idle.value?.id, JSON.stringify(idle.value))

  console.log('=== 2. cancel 不存在的任务 ===')
  const cancelGhost = await rpc('cancel', { id: 'dl-not-exist' })
  ok('返回 ok（不报错）', cancelGhost.ok === true, JSON.stringify(cancelGhost))
  ok('cancelled 为 false', cancelGhost.value?.cancelled === false, JSON.stringify(cancelGhost.value))

  console.log('=== 3. 进度增长 + 中途取消 ===')
  // 触发 stage 下载（如果已是最新会立刻返回，此时跳过进度用例）
  let stageSettled = false
  const stagePromise = rpc('update', { step: 'stage' }).finally(() => { stageSettled = true })
  let sawProgress = false
  const progressSamples = []
  let sawCancelOk = false

  for (let i = 0; i < 40; i++) {
    await sleep(200)
    try {
      const p = await rpc('progress', {})
      if (p.ok && p.value && !p.value.idle && p.value.id) {
        progressSamples.push({ received: p.value.received, percent: p.value.percent })
        if (p.value.received > 0) sawProgress = true
        // 第一次看到有进度就发取消
        if (!sawCancelOk && sawProgress) {
          const c = await rpc('cancel', { id: p.value.id })
          sawCancelOk = c.ok && c.value?.cancelled === true
        }
      }
    } catch {}
    // stage 已结束则停止轮询
    if (stageSettled) break
  }

  const stage = await stagePromise
  if (sawProgress) {
    ok('轮询能看到字节增长', sawProgress, JSON.stringify(progressSamples.slice(0, 3)))
    ok('cancel 请求被接受', sawCancelOk)
    ok('stage 因取消而失败', stage.ok === false && stage.error?.code === 'aborted',
       JSON.stringify({ ok: stage.ok, code: stage.error?.code }))
  } else {
    console.log('  （下载在轮询窗口内已完成或已是最新，跳过进度用例）')
    console.log('  stage 结果：', JSON.stringify({ ok: stage.ok, code: stage.error?.code, staged: stage.value?.staged }))
  }

  console.log('')
  console.log(`结果：${passed} 通过 / ${failed} 失败`)
  process.exit(failed > 0 ? 1 : 0)
}

main().catch((e) => {
  console.error('脚本异常：', e)
  process.exit(1)
})
