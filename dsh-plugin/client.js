/**
 * dsh-multiver-bridge —— Client 半（M2 / B3）。
 *
 * 职责边界（计划 §1.3）：一眼看清状态 + 一次性小决策 + 把你送回 multiver。
 * 不做安装/卸载/隔离/维护/更新，不做任何进程编排。
 *
 * locale：slot 的 label 是 registrant-localized，shell 不订阅 locale，
 * 故订阅 ctx.locale，变化时 dispose 旧注册再注册。
 */
window.__ModuleLoader__.load({
  id: 'dsh-multiver-bridge',
  factory(require) {
    const React = require('react')
    const h = React.createElement
    const NS = 'multiver-bridge'
    const CH = '/multiver-bridge'

    // 设置页导航图标替换（尽力而为，失败静默）
    // 三条横杠 + 右下圆点：多版本堆叠的隐喻，尺寸 16×16，跟随 currentColor。
    function installNavIcon(getLabel) {
      const ICON_HTML =
        '<path d="M2.5 4.5h11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>' +
        '<path d="M2.5 8h11" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>' +
        '<path d="M2.5 11.5h6.5" stroke="currentColor" stroke-width="1.3" stroke-linecap="round"/>' +
        '<circle cx="12.4" cy="11.5" r="1.5" fill="currentColor"/>'

      const swap = () => {
        const label = getLabel()
        if (!label) return
        let done = 0
        document.querySelectorAll('nav button').forEach((btn) => {
          if ((btn.textContent || '').trim() !== label) return
          const svg = btn.querySelector('svg')
          if (!svg || svg.dataset.multiverIcon === '1') return
          try {
            svg.innerHTML = ICON_HTML
            svg.dataset.multiverIcon = '1'
            done++
          } catch {}
        })
        return done
      }

      swap()

      let timer = null
      const obs = new MutationObserver(() => {
        if (timer) return
        timer = setTimeout(() => { timer = null; swap() }, 120)
      })
      obs.observe(document.body, { childList: true, subtree: true })

      return () => {
        if (timer) clearTimeout(timer)
        obs.disconnect()
      }
    }

    function rpc(endpoint, payload) {
      return fetch(CH + '/' + endpoint, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(payload ?? {}),
        credentials: 'same-origin',
      }).then(async (res) => {
        if (res.status === 401) throw new Error('unauthorized')
        if (res.status === 403) throw new Error('forbidden')
        const body = await res.json().catch(() => null)
        if (!body) throw new Error('bad-response')
        return body
      })
    }

    /// 下载进度轮询。
    ///
    /// 插件侧没有服务端推送通道（只有 HTTP 请求-响应），所以进度靠轮询 progress 端点取得。
    /// 下载请求本身是阻塞的 fetch，本 hook 与它并发运行：active 为 true 时开轮询，
    /// 变回 false 时停止。
    ///
    /// 返回 { task, cancel, cancelling }：
    ///   task        最近一次拿到的任务状态（含 percent / received / total / elapsedMs），无任务时为 null
    ///   cancel()    请求取消当前任务
    ///   cancelling  已发出取消请求、等待生效
    function useDownloadProgress(active) {
      const [task, setTask] = React.useState(null)
      const [cancelling, setCancelling] = React.useState(false)
      const taskIdRef = React.useRef(null)

      React.useEffect(() => {
        if (!active) {
          setTask(null)
          setCancelling(false)
          taskIdRef.current = null
          return
        }
        let stopped = false
        let timer = null
        const tick = async () => {
          if (stopped) return
          try {
            const r = await rpc('progress', {})
            if (stopped) return
            const v = r?.ok ? r.value : null
            if (v && !v.idle) {
              taskIdRef.current = v.id
              setTask(v)
            }
          } catch {}
          if (!stopped) timer = setTimeout(tick, 400)
        }
        tick()
        return () => {
          stopped = true
          if (timer) clearTimeout(timer)
        }
      }, [active])

      const cancel = React.useCallback(async () => {
        const id = taskIdRef.current
        if (!id) return
        setCancelling(true)
        try { await rpc('cancel', { id }) } catch {}
      }, [])

      return { task, cancel, cancelling }
    }

    /// 字节数转可读大小。
    function fmtBytes(n) {
      if (!Number.isFinite(n) || n <= 0) return '0 B'
      const units = ['B', 'KB', 'MB', 'GB']
      let i = 0
      let v = n
      while (v >= 1024 && i < units.length - 1) { v /= 1024; i++ }
      return (i === 0 ? v : v.toFixed(1)) + ' ' + units[i]
    }

    /// 已用时长（秒）。
    function fmtElapsed(ms) {
      const s = Math.max(0, Math.floor((ms || 0) / 1000))
      return s < 60 ? s + 's' : Math.floor(s / 60) + 'm' + (s % 60) + 's'
    }

    // 部署向导（阶段 2）：provisioned=false 时替代全部功能 UI。
    // 三步：选工作目录 → 获取 exe（下载/已有）→ 完成并启动。
    function ProvisionWizard(props) {
      const t = React.useContext(LocaleCtx) ?? ((k) => k)
      const info = props.provision || {}
      const [step, setStep] = React.useState(1)
      const [workdir, setWorkdir] = React.useState(info.defaultWorkdir || '')
      const [adoptPath, setAdoptPath] = React.useState('')
      const [mode, setMode] = React.useState('download') // download | adopt
      const [busy, setBusy] = React.useState(false)
      const [err, setErr] = React.useState(null)
      const [result, setResult] = React.useState(null)
      // 仅在下载阶段轮询进度，避免无谓请求
      const dl = useDownloadProgress(busy && mode === 'download' && step === 2)

      const run = React.useCallback(async (payload, onOk) => {
        setBusy(true)
        setErr(null)
        try {
          const r = await rpc('provision', payload)
          if (r.ok) { onOk(r.value) } else { setErr(r.error) }
        } catch (e) {
          setErr({ code: 'network', message: String(e?.message ?? e) })
        } finally {
          setBusy(false)
        }
      }, [])

      const doAcquire = React.useCallback(() => {
        if (mode === 'adopt') {
          run({ step: 'adopt', exe: adoptPath }, (v) => {
            setResult({ exe: v.exe, verified: false, adopted: true })
            setStep(3)
          })
        } else {
          run({ step: 'download', workdir }, (v) => {
            setResult(v)
            setStep(3)
          })
        }
      }, [mode, adoptPath, workdir, run])

      const doFinish = React.useCallback(() => {
        const exe = result?.exe
        run({ step: 'setRoot', exe, workdir }, () => {
          run({ step: 'launch' }, () => { props.onDone && props.onDone() })
        })
      }, [result, workdir, run, props])

      const errText = (e) => {
        if (!e) return null
        const key = 'err.' + e.code
        const mapped = t(key)
        return (mapped && mapped !== key ? mapped : e.code) + (e.message ? '：' + e.message : '')
      }

      const box = { padding: '16px 20px', maxWidth: 720 }
      const card = {
        border: '1px solid rgba(128,128,128,.25)', borderRadius: 8,
        padding: '14px 16px', marginBottom: 12,
      }
      const inputStyle = {
        width: '100%', padding: '7px 10px', borderRadius: 6, fontSize: 13,
        border: '1px solid rgba(128,128,128,.35)', background: 'transparent',
        color: 'inherit', fontFamily: 'ui-monospace, monospace',
      }
      const rowGap = { display: 'flex', gap: 8, marginTop: 10, flexWrap: 'wrap' }

      return h('div', { style: box },
        h('div', { style: { fontSize: 15, fontWeight: 600, marginBottom: 4 } }, t('provision.title')),
        h('div', { style: { fontSize: 13, opacity: .7, marginBottom: 16 } }, t('provision.subtitle')),

        // 步骤指示
        h('div', { style: { display: 'flex', gap: 18, fontSize: 12, marginBottom: 16, opacity: .8 } },
          [1, 2, 3].map((n) => h('span', {
            key: n,
            style: { fontWeight: step === n ? 700 : 400, opacity: step >= n ? 1 : .5 },
          }, `${n}. ${t('provision.step' + n)}`))
        ),

        // 步骤 1：选工作目录
        step === 1 && h('div', { style: card },
          h('div', { style: { fontSize: 13, marginBottom: 8 } }, t('provision.pickDirHint')),
          h('input', {
            style: inputStyle,
            value: workdir,
            placeholder: info.defaultWorkdir || 'D:/dsh-multiver',
            onChange: (e) => setWorkdir(e.target.value),
            disabled: busy,
          }),
          h('div', { style: rowGap },
            h('button', {
              style: btnStyle,
              disabled: busy || !workdir.trim(),
              onClick: () => { setErr(null); setStep(2) },
            }, t('provision.next'))
          )
        ),

        // 步骤 2：获取 exe
        step === 2 && h('div', { style: card },
          h('div', { style: { display: 'flex', gap: 14, marginBottom: 10, fontSize: 13 } },
            h('label', { style: { display: 'flex', gap: 5, alignItems: 'center', cursor: 'pointer' } },
              h('input', { type: 'radio', checked: mode === 'download', onChange: () => setMode('download'), disabled: busy }),
              t('provision.modeDownload')),
            h('label', { style: { display: 'flex', gap: 5, alignItems: 'center', cursor: 'pointer' } },
              h('input', { type: 'radio', checked: mode === 'adopt', onChange: () => setMode('adopt'), disabled: busy }),
              t('provision.modeAdopt'))
          ),
          mode === 'download'
            ? h('div', null,
                h('div', { style: { fontSize: 12, opacity: .7, marginBottom: 6 } },
                  t('provision.downloadHint', { repo: info.releaseRepo || '' })),
                h('div', { style: { fontSize: 11, fontFamily: 'ui-monospace, monospace', opacity: .6, wordBreak: 'break-all' } },
                  workdir + '/' + (info.platform === 'win32' ? 'dsh-multiver.exe' : 'dsh-multiver')))
            : h('div', null,
                h('div', { style: { fontSize: 12, opacity: .7, marginBottom: 6 } }, t('provision.adoptHint')),
                h('input', {
                  style: inputStyle,
                  value: adoptPath,
                  placeholder: 'D:/tools/dsh-multiver.exe',
                  onChange: (e) => setAdoptPath(e.target.value),
                  disabled: busy,
                })),
          busy && mode === 'download' && h('div', { style: { marginTop: 12 } },
            h('div', { style: { height: 4, borderRadius: 2, background: 'rgba(128,128,128,.22)', overflow: 'hidden' } },
              h('div', {
                style: {
                  height: '100%',
                  width: (dl.task?.percent ?? 0) + '%',
                  background: 'currentColor',
                  opacity: .55,
                  transition: 'width .25s linear',
                },
              })),
            h('div', { style: { display: 'flex', justifyContent: 'space-between', fontSize: 11.5, opacity: .7, marginTop: 5 } },
              h('span', null,
                dl.task
                  ? (dl.task.percent == null
                      ? fmtBytes(dl.task.received) + ' / ?'
                      : dl.task.percent + '% · ' + fmtBytes(dl.task.received) + ' / ' + fmtBytes(dl.task.total))
                  : t('download.connecting')),
              h('span', null, fmtElapsed(dl.task?.elapsedMs ?? 0)))),
          busy && mode === 'adopt' && h('div', { style: { marginTop: 10, fontSize: 12, opacity: .75 } }, t('provision.working')),
          err && h('div', {
            style: { marginTop: 10, padding: '8px 12px', borderRadius: 6, background: 'rgba(220,50,50,.12)', fontSize: 12 },
          }, errText(err)),
          h('div', { style: rowGap },
            h('button', { style: btnStyle, disabled: busy, onClick: () => { setErr(null); setStep(1) } }, t('provision.back')),
            busy && mode === 'download' && h('button', {
              style: btnStyle,
              disabled: dl.cancelling,
              onClick: dl.cancel,
            }, dl.cancelling ? t('download.cancelling') : t('download.cancel')),
            h('button', {
              style: { ...btnStyle, opacity: busy || (mode === 'adopt' ? !adoptPath.trim() : !workdir.trim()) ? .5 : 1 },
              disabled: busy || (mode === 'adopt' ? !adoptPath.trim() : !workdir.trim()),
              onClick: doAcquire,
            }, mode === 'download' ? t('provision.doDownload') : t('provision.doAdopt'))
          )
        ),

        // 步骤 3：完成并启动
        step === 3 && h('div', { style: card },
          h('div', { style: { fontSize: 13, marginBottom: 8 } }, t('provision.acquired')),
          h('div', { style: { fontSize: 11.5, fontFamily: 'ui-monospace, monospace', opacity: .75, wordBreak: 'break-all' } }, result?.exe || ''),
          result?.verified && h('div', { style: { fontSize: 12, color: '#2a9d3f', marginTop: 6 } },
            t('provision.checksumOk')),
          result?.adopted && h('div', { style: { fontSize: 12, opacity: .7, marginTop: 6 } },
            t('provision.adoptNoChecksum')),
          err && h('div', {
            style: { marginTop: 10, padding: '8px 12px', borderRadius: 6, background: 'rgba(220,50,50,.12)', fontSize: 12 },
          }, errText(err)),
          h('div', { style: rowGap },
            h('button', { style: btnStyle, disabled: busy, onClick: () => { setErr(null); setResult(null); setStep(2) } }, t('provision.back')),
            h('button', { style: { ...btnStyle, fontWeight: 600 }, disabled: busy, onClick: doFinish },
              busy ? t('provision.working') : t('provision.finish'))
          )
        ),

        info.reasons && info.reasons.length > 0 && step === 1 && h('div', {
          style: { marginTop: 8, fontSize: 11.5, opacity: .55 },
        }, t('provision.reasonPrefix') + ' ' + info.reasons.join(', '))
      )
    }

    // 更新管理块（阶段 3）：检查 → 显示 → 用户确认 → 委托 multiver update。
    function UpdateBlock(props) {
      const t = React.useContext(LocaleCtx) ?? ((k) => k)
      const [info, setInfo] = React.useState(null)
      const [busy, setBusy] = React.useState(false)
      const [msg, setMsg] = React.useState(null)
      const [err, setErr] = React.useState(null)
      const [confirming, setConfirming] = React.useState(false)
      // 暂存更新会下载约 10 MB：期间轮询进度，让用户看到「还要多久」
      const dl = useDownloadProgress(busy)

      const check = React.useCallback(async () => {
        setBusy(true); setErr(null); setMsg(null)
        try {
          const r = await rpc('update', { step: 'check' })
          if (r.ok) setInfo(r.value)
          else setErr(r.error)
        } catch (e) {
          setErr({ code: 'network', message: String(e?.message ?? e) })
        } finally { setBusy(false) }
      }, [])

      React.useEffect(() => { check() }, [check])

      // 内嵌模式下 apply 会被 Host 自动降级为 stage（暂存，下次启动安装）。
      // 两种结果都要能正确展示。
      const apply = React.useCallback(async () => {
        setConfirming(false); setBusy(true); setErr(null); setMsg(null)
        try {
          const r = await rpc('update', { step: 'apply' })
          if (r.ok) {
            const v = r.value || {}
            if (v.deferred) {
              // 内嵌模式：已暂存
              setMsg(v.message || t('update.staged'))
              setInfo((s) => s ? { ...s, hasUpdate: false, pendingUpdate: { version: v.version } } : s)
            } else if (v.upToDate) {
              setMsg(t('update.upToDate'))
            } else {
              setMsg(v.message || t('update.handedOff'))
            }
            setInfo((s) => s ? { ...s, hasUpdate: false } : s)
          } else {
            setErr(r.error)
          }
        } catch (e) {
          setErr({ code: 'network', message: String(e?.message ?? e) })
        } finally { setBusy(false) }
      }, [t])

      const errText = (e) => {
        if (!e) return null
        const key = 'err.' + e.code
        const mapped = t(key)
        return (mapped && mapped !== key ? mapped : e.code) + (e.message ? '：' + e.message : '')
      }

      const row = { display: 'flex', alignItems: 'center', gap: 8, flexWrap: 'wrap', fontSize: 12 }
      const btn = (primary) => ({
        fontSize: 11, padding: '3px 10px', borderRadius: 6, cursor: busy ? 'not-allowed' : 'pointer',
        border: '1px solid ' + (primary ? 'transparent' : 'rgba(128,128,128,.35)'),
        background: primary ? '#247bbf' : 'transparent',
        color: primary ? '#fff' : 'inherit',
        opacity: busy ? 0.5 : 1,
      })

      return h('div', {
        style: {
          marginTop: 16, paddingTop: 12, borderTop: '1px solid rgba(128,128,128,.18)',
        },
      },
        h('div', { style: { ...row, marginBottom: 6 } },
          h('span', { style: { fontWeight: 600 } }, t('update.title')),
          info && info.current && h('span', { style: { opacity: .65 } },
            t('update.current', { v: info.current })),
          busy && h('span', { style: { opacity: .6 } },
            dl.task ? t('update.workingWithTime', { s: fmtElapsed(dl.task.elapsedMs) }) : t('update.working'))
        ),
        // 更新期间不可取消：multiver 的 update 是阻塞子进程，中途终止会留下半替换的 exe
        busy && info && info.hasUpdate && h('div', { style: { fontSize: 11.5, opacity: .6, marginBottom: 4 } },
          t('update.noCancelHint')),
        !info && !err && !busy && h('div', { style: { fontSize: 12, opacity: .6 } }, t('update.checking')),
        info && !info.hasUpdate && h('div', { style: { ...row } },
          h('span', { style: { opacity: .75 } }, info.error
            ? t('update.checkFailed', { e: info.error })
            : t('update.upToDate')),
          h('button', { style: btn(false), disabled: busy, onClick: check }, t('update.recheck'))
        ),
        info && info.hasUpdate && h('div', { style: { ...row } },
          h('span', { style: { fontWeight: 600, color: '#2a9d3f' } },
            t('update.available', { v: info.latest })),
          info.releaseUrl && h('a', {
            href: info.releaseUrl, target: '_blank', rel: 'noreferrer',
            style: { fontSize: 11, opacity: .7 },
          }, t('update.changelog')),
          confirming
            ? h('span', { style: { ...row } },
                h('span', { style: { opacity: .85 } }, t('update.confirm')),
                h('button', { style: btn(true), disabled: busy, onClick: apply }, t('update.yes')),
                h('button', { style: btn(false), disabled: busy, onClick: () => setConfirming(false) }, t('update.no'))
              )
            : h('button', { style: btn(true), disabled: busy, onClick: () => setConfirming(true) }, t('update.doUpdate'))
        ),
        // 内嵌窗口下点更新走「暂存」，multiver 下次启动时自动安装，不打断当前会话
        info && info.hasUpdate && info.runningInMultiver && h('div', {
          style: { marginTop: 6, fontSize: 11.5, opacity: .7, lineHeight: '18px' },
        }, t('update.embeddedHint')),
        // 已暂存待安装的更新
        info && info.pendingUpdate && h('div', {
          style: { marginTop: 8, padding: '6px 10px', borderRadius: 6, background: 'rgba(36,123,191,.10)', fontSize: 12 },
        }, t('update.pendingInstall', { v: info.pendingUpdate.version })),
        msg && h('div', { style: { marginTop: 6, fontSize: 12, color: '#2a9d3f' } }, msg),
        err && h('div', {
          style: { marginTop: 6, padding: '6px 10px', borderRadius: 6, background: 'rgba(220,50,50,.12)', fontSize: 12 },
        }, errText(err))
      )
    }

    function VersionsPage(props) {
      const t = React.useContext(LocaleCtx) ?? ((k) => k)
      const [state, setState] = React.useState({ kind: 'loading' })
      const [pending, setPending] = React.useState(null)
      const [confirming, setConfirming] = React.useState(null)
      const [copyFallback, setCopyFallback] = React.useState(null)
      const [actionMsg, setActionMsg] = React.useState(null)

      // 送回 multiver 的三个动作统一在这里处理：
      //  成功 → 清空提示；失败 → 显示可读错误（含后端错误码映射）
      const runAction = React.useCallback(async (fn, okText) => {
        try {
          await fn()
          setActionMsg(okText)
          setCopyFallback(null)
        } catch (e) {
          setActionMsg(String(e?.message ?? e))
        }
      }, [])

      const load = React.useCallback(() => {
        setState((s) => (s.kind === 'ok' ? s : { kind: 'loading' }))
        rpc('state', {}).then((r) => {
          if (r.ok) setState({ kind: 'ok', value: r.value })
          else setState({ kind: 'error', error: r.error })
        }).catch((e) => setState({ kind: 'error', error: { code: 'network', message: String(e.message ?? e) } }))
      }, [])

      // 初始加载
      React.useEffect(() => { load() }, [load])

      // M3a：刷新策略
      //  - 页面可见时每 60 秒刷新一次
      //  - 页面从隐藏变可见（visibilitychange）时立即刷新一次
      //  - cleanup 必须清 timer 与事件监听，避免切换页面后仍在跑
      //  - 不在隐藏时轮询：每次刷新都要 spawn 一次 exe，代价不低
      React.useEffect(() => {
        const REFRESH_MS = 60000
        let timer = null

        const start = () => {
          if (timer) return
          timer = setInterval(load, REFRESH_MS)
        }
        const stop = () => {
          if (timer) { clearInterval(timer); timer = null }
        }

        const onVisibility = () => {
          if (document.visibilityState === 'visible') {
            load()
            start()
          } else {
            stop()
          }
        }

        if (document.visibilityState === 'visible') start()
        document.addEventListener('visibilitychange', onVisibility)

        return () => {
          stop()
          document.removeEventListener('visibilitychange', onVisibility)
        }
      }, [load])

      const doSetDefault = React.useCallback((version) => {
        setPending(version)
        setConfirming(null)
        rpc('setDefault', { version }).then((r) => {
          setPending(null)
          if (r.ok) {
            load()
            setActionMsg(t('switched', { version }))
          } else {
            setState((s) => (s.kind === 'ok' ? { ...s, actionError: r.error } : s))
          }
        }).catch((e) => {
          setPending(null)
          setState((s) => (s.kind === 'ok' ? { ...s, actionError: { code: 'network', message: String(e.message ?? e) } } : s))
        })
      }, [load, t])

      if (state.kind === 'loading') {
        return h('div', { style: { padding: 16, opacity: 0.7 } }, t('loading'))
      }

      if (state.kind === 'error') {
        return h('div', { style: { padding: 16 } },
          h('div', { style: { fontWeight: 600, marginBottom: 8 } }, t('err.' + state.error.code) || state.error.code),
          h('div', { style: { opacity: 0.75, marginBottom: 12, fontSize: 13 } }, state.error.message),
          h('button', { onClick: load, style: btnStyle }, t('retry'))
        )
      }

      // 阶段 2 门控：未配置数据根时只显示部署向导，不渲染版本表与任何功能按钮
      if (!state.value.provisioned) {
        return h(ProvisionWizard, {
          provision: state.value.provision,
          onDone: load,
        })
      }

      const v = state.value

      // M3a：校对详情。只列 mismatch / unknown 的项，ok 不占篇幅。
      const consistencyIssues = (c) => {
        if (!c) return []
        return [
          ['defaultEntry', t('consistencyDefaultEntry')],
          ['root', t('consistencyRoot')],
          ['exe', t('consistencyExe')],
        ]
          .filter(([k]) => c[k] === 'mismatch' || c[k] === 'unknown')
          .map(([k, label]) => label + t('consistencySep') + (c[k] === 'mismatch' ? t('consistencyBad') : t('consistencyUnknown')))
      }
      // 只要有一项 mismatch 就展开详情；全 ok / 全 unknown 时不占版面
      const hasConsistencyMismatch = (c) => !!c && ['defaultEntry', 'root', 'exe'].some((k) => c[k] === 'mismatch')

      return h('div', { style: { padding: '16px 20px' } },
        // 状态条
        h('div', { style: { marginBottom: 16 } },
          row(t('defaultLabel'), v.defaultVersion ?? '—'),
          row(t('runningLabel'), v.runningVersion
            ? [v.runningVersion, confBadge(t, v.confidence)]
            : t('unknownVersion')),
          // A1：Client 侧也按 confidence 门控，不只依赖 Host 的 pendingRestart 字段
          v.pendingRestart
            && v.confidence !== 'conflict'
            && v.confidence !== 'unknown'
            && h('div', {
              style: { marginTop: 8, padding: '8px 12px', borderRadius: 6, background: 'rgba(255,170,0,.12)', fontSize: 13 },
            }, t('pendingRestart', { running: v.runningVersion, next: v.defaultVersion })),
          // M3a：conflict 态——两个来源不一致，先引导看校对，不误报「待重启」
          v.confidence === 'conflict' && h('div', {
            style: { marginTop: 8, padding: '8px 12px', borderRadius: 6, background: 'rgba(220,50,50,.12)', fontSize: 13 },
          },
            h('div', { style: { fontWeight: 600 } }, t('conflictTitle')),
            h('div', { style: { opacity: 0.85, marginTop: 2 } }, t('conflictHint'))
          ),
          // 兼容性提示：仅在「运行版本已知且不在已验证列表」时显示，不阻断功能
          v.compatibility
            && v.compatibility.verified === false
            && h('div', {
              style: { marginTop: 8, padding: '8px 12px', borderRadius: 6, background: 'rgba(255,170,0,.10)', fontSize: 13, opacity: 0.9 },
            }, t('compatUnverified', {
              running: v.compatibility.runningVersion,
              list: (v.compatibility.verifiedVersions || []).join(' / ') || '—',
            })),
          // M3a：一致性校对详情（有 mismatch 才展开）
          hasConsistencyMismatch(v.consistency) && h('div', {
            style: { marginTop: 8, padding: '8px 12px', borderRadius: 6, background: 'rgba(220,50,50,.12)', fontSize: 13 },
          },
            h('div', { style: { fontWeight: 600, marginBottom: 4 } }, t('consistencyMismatch')),
            h('ul', { style: { margin: '4px 0 0', paddingLeft: 18, opacity: 0.85 } },
              consistencyIssues(v.consistency).map((line, i) => h('li', { key: i }, line))
            )
          )
        ),
        // E3：空态——multiver 里没有任何已安装版本时，不显示空表头，改给引导文案
        v.versions.length === 0 && h('div', {
          style: { padding: '24px 16px', textAlign: 'center', opacity: 0.75, fontSize: 14, lineHeight: '22px' },
        },
          h('div', { style: { fontWeight: 600, marginBottom: 6 } }, t('emptyTitle')),
          h('div', null, t('emptyHint')),
          h('div', { style: { marginTop: 8, fontSize: 12, opacity: 0.7, lineHeight: '19px' } }, t('emptyNote'))
        ),
        // 版本表
        v.versions.length > 0 && h('table', { style: { width: '100%', borderCollapse: 'collapse', fontSize: 14 } },
          h('thead', null, h('tr', { style: { textAlign: 'left', opacity: 0.6, fontSize: 12 } },
            h('th', { style: thStyle }, t('col.version')),
            h('th', { style: thStyle }, t('col.installedAt')),
            h('th', { style: thStyle }, t('col.flags')),
            h('th', { style: thStyle }, '')
          )),
          h('tbody', null, v.versions.map((it) => h('tr', { key: it.version, style: { borderTop: '1px solid rgba(128,128,128,.18)' } },
            h('td', { style: tdStyle },
              h('span', { style: { fontFamily: 'ui-monospace, monospace' } }, it.version),
              it.isDefault && h('span', { style: badgeStyle }, t('badge.default'))
            ),
            h('td', { style: tdStyle }, it.installedAt ?? '—'),
            h('td', { style: tdStyle },
              it.isolated && h('span', { style: badgeStyle }, t('badge.isolated')),
              it.sharedHome && h('span', { style: { ...badgeStyle, opacity: 0.6 } }, t('badge.sharedHome'))
            ),
            h('td', { style: { ...tdStyle, textAlign: 'right' } },
              it.isDefault ? null : (confirming === it.version
                ? h('span', null,
                    h('span', { style: { fontSize: 12, marginRight: 8 } }, t('confirmSwitch')),
                    h('button', { onClick: () => doSetDefault(it.version), disabled: !!pending, style: btnStyle }, t('yes')),
                    h('button', { onClick: () => setConfirming(null), style: { ...btnStyle, marginLeft: 6 } }, t('no'))
                  )
                : h('button', {
                    onClick: () => setConfirming(it.version),
                    disabled: !!pending,
                    style: btnStyle,
                  }, pending === it.version ? t('switching') : t('setDefault'))
              )
            )
          )))
        ),
        state.actionError && h('div', {
          style: { marginTop: 12, padding: '8px 12px', borderRadius: 6, background: 'rgba(220,50,50,.12)', fontSize: 13 },
        },
          h('div', { style: { fontWeight: 600 } }, t('err.' + state.actionError.code) || state.actionError.code),
          h('div', { style: { opacity: 0.8 } }, state.actionError.message)
        ),
        // 送回 multiver：三个一次性动作
        h('div', { style: { marginTop: 20, display: 'flex', gap: 8, flexWrap: 'wrap' } },
          h('button', {
            onClick: () => runAction(
              () => sendAction(t, 'open', { target: 'multiver-dir' }),
              t('openMultiverDir') + ' ✓'
            ),
            style: btnStyle,
          }, t('openMultiverDir')),
          h('button', {
            onClick: () => runAction(
              () => sendAction(t, 'open', { target: 'version-dir', version: v.defaultVersion }),
              t('openVersionDir') + ' ✓'
            ),
            disabled: !v.defaultVersion,
            style: btnStyle,
          }, t('openVersionDir')),
          // G1：console 模式下「回到 multiver」入口；embedded 窗口里点它会再开一个管理器，故隐藏
          v.mode !== 'embedded' && h('button', {
            onClick: () => runAction(
              () => sendAction(t, 'open', { target: 'launch-multiver' }),
              t('launchMultiver') + ' ✓'
            ),
            style: btnStyle,
          }, t('launchMultiver')),
          h('button', {
            onClick: () => runAction(
              () => sendAction(t, 'open', { target: 'multiver-settings' }),
              t('openMultiverSettings') + ' ✓'
            ),
            style: btnStyle,
          }, t('openMultiverSettings')),
          h('button', {
            onClick: async () => {
              const r = await copyDiagnostics(v)
              setCopyFallback(r.ok ? null : r.text)
              setActionMsg(r.ok ? t('copyOk') : null)
            },
            style: btnStyle,
          }, t('copyDiagnostics'))
        ),
        copyFallback && h('div', { style: { marginTop: 8 } },
          h('div', { style: { fontSize: 12, opacity: 0.7, marginBottom: 4 } }, t('copyFallbackHint')),
          h('textarea', {
            readOnly: true,
            value: copyFallback,
            style: { width: '100%', height: 96, fontFamily: 'ui-monospace, monospace', fontSize: 11 },
            onFocus: (e) => e.target.select(),
          })
        ),
        // 阶段 3：multiver 自身更新管理。委托 multiver 自带 updater，插件不重复实现替换。
        h(UpdateBlock, { value: v }),
        // 页脚
        h('div', { style: { marginTop: 16, paddingTop: 12, borderTop: '1px solid rgba(128,128,128,.18)', fontSize: 12, opacity: 0.65 } },
          h('div', { style: { marginBottom: 4 } }, t('exePath', { exe: v.exe }) + ' · ' + t('detectedBy', { by: v.detectedBy })),
          h('button', { onClick: load, style: { ...btnStyle, fontSize: 12 } }, t('refresh'))
        ),
        actionMsg && h('div', {
          style: { marginTop: 8, fontSize: 12, opacity: 0.75 },
        }, actionMsg)
      )
    }

    async function sendAction(t, endpoint, payload) {
      const r = await rpc(endpoint, payload)
      if (!r.ok) {
        const key = 'err.' + r.error.code
        const localized = t(key)
        // t() 未命中时原样返回 key，此时退回到后端 message
        throw new Error(localized === key ? (r.error.message || r.error.code) : localized)
      }
      return r.value
    }

    async function copyDiagnostics(v) {
      const text = JSON.stringify({
        exe: v.exe,
        detectedBy: v.detectedBy,
        defaultVersion: v.defaultVersion,
        runningVersion: v.runningVersion,
        confidence: v.confidence,
        pendingRestart: v.pendingRestart,
        consistency: v.consistency,
        versions: v.versions,
        fetchedAt: v.fetchedAt,
      }, null, 2)
      try {
        await navigator.clipboard.writeText(text)
        return { ok: true, text }
      } catch {
        return { ok: false, text }
      }
    }

    const LocaleCtx = React.createContext(null)

    const btnStyle = {
      padding: '4px 10px', borderRadius: 6, fontSize: 13,
      border: '1px solid rgba(128,128,128,.35)', background: 'transparent',
      color: 'inherit', cursor: 'pointer',
    }
    const thStyle = { padding: '6px 8px', fontWeight: 500 }
    const tdStyle = { padding: '8px' }
    const badgeStyle = {
      display: 'inline-block', marginLeft: 8, padding: '1px 6px',
      borderRadius: 4, fontSize: 11, background: 'rgba(128,128,128,.18)',
    }

    function row(label, value) {
      const content = Array.isArray(value) ? value : [value]
      return h('div', { style: { display: 'flex', gap: 8, fontSize: 13, lineHeight: '22px' } },
        h('span', { style: { opacity: 0.6, minWidth: 96 } }, label),
        h('span', { style: { fontFamily: 'ui-monospace, monospace' } }, content)
      )
    }

    function confBadge(t, confidence) {
      if (!confidence || confidence === 'high') return null
      return h('span', { style: { ...badgeStyle, marginLeft: 6 } }, t('conf.' + confidence))
    }

    return {
      inject: ['slots', 'locale'],
      apply(ctx) {
        const t = ctx.locale.bind(NS)

        ctx.effect(() => ctx.locale.register(NS, {
          zh: {
            section: '版本管理',
            title: 'dsh-multiver 版本管理',
            loading: '正在读取版本信息…',
            retry: '重试',
            refresh: '刷新',
            defaultLabel: '默认版本',
            runningLabel: '运行版本',
            unknownVersion: '无法确定',
            pendingRestart: '已设为 {next}；当前运行 {running}，重启 dsh 后生效。',
            consistencyMismatch: '终端 dsh 入口与默认版本不一致（shim 可能损坏）。请在 multiver 重新执行一次「设为默认」。',
            'col.version': '版本',
            'col.installedAt': '安装日期',
            'col.flags': '标记',
            'badge.default': '默认',
            'badge.isolated': '隔离',
            'badge.sharedHome': '共享 home',
            setDefault: '设为默认',
            switching: '设置中…',
            confirmSwitch: '重启后生效，确认？',
            yes: '确认',
            no: '取消',
            switched: '已设为 {version}，重启 dsh 后生效。',
            exePath: 'multiver：{exe}',
            detectedBy: '来源：{by}',
            'conf.low': '推断',
            'conf.medium': '来自 multiver',
            'conf.conflict': '来源冲突',
            'conf.unknown': '未知',
            'err.exe-not-found': '找不到 dsh-multiver.exe',
            'err.spawn-failed': '无法启动 dsh-multiver',
            'err.timeout': 'dsh-multiver 响应超时',
            'err.json-invalid': 'multiver 版本过旧，需 ≥0.3.0',
            'err.cli-error': 'dsh-multiver 返回错误',
            'err.version-not-installed': '该版本未安装',
            'err.invalid-version': '版本号格式不正确',
            'err.unknown-endpoint': '未知的接口',
            'err.internal': '内部错误',
            emptyTitle: 'multiver 还没有安装任何 dsh 版本',
            emptyHint: '请在 dsh-multiver 里安装一个版本，再回到这里管理。',
            emptyNote: 'multiver 只管它自己装的那份 dsh（位于数据根的 versions/ 目录）。你通过 npm、pnpm、Python wheel 或 DeepSeek Harness 桌面版装的 dsh 是独立的一份，不受本插件影响，也不会出现在这个列表里。',
            'err.network': '网络或鉴权失败',
            'err.invalid-target': '目标不受支持',
            'err.outside-root': '路径超出数据根',
            'err.open-failed': '打开失败',
            'err.launch-failed': '启动 multiver 失败',
            launchMultiver: '打开 multiver',
            openMultiverDir: '打开 multiver 目录',
            openMultiverSettings: '打开 multiver 路径设置',
            openVersionDir: '打开版本目录',
            copyDiagnostics: '复制诊断信息',
            copyFallbackHint: '剪贴板不可用，请手动复制以下内容：',
            copyOk: '诊断信息已复制到剪贴板',
            conflictTitle: '启动来源不一致',
            conflictHint: 'multiver 记录的启动版本与实际加载的版本不一致。请重启 dsh，或查看下方校对详情。',
            consistencyDefaultEntry: '默认版本与终端入口',
            consistencyRoot: '数据根与 exe 位置',
            consistencyExe: 'exe 实际来源',
            consistencySep: '：',
            consistencyBad: '不一致',
            consistencyUnknown: '无法确定',
            compatUnverified: '当前 dsh {running} 未经验证。插件按 {list} 验证，功能可能在后续版本失效；如遇问题请在 multiver 侧反馈。',
            'provision.title': '首次使用：部署 dsh-multiver',
            'provision.subtitle': '需要先准备好 dsh-multiver.exe 并选择工作目录，之后即可在这里管理 dsh 版本。',
            'provision.step1': '选择工作目录',
            'provision.step2': '获取 dsh-multiver.exe',
            'provision.step3': '完成并启动',
            'provision.pickDirHint': '选择存放版本与配置的目录（不存在会自动创建，不会使用系统目录）：',
            'provision.next': '下一步',
            'provision.back': '上一步',
            'provision.modeDownload': '自动下载最新版',
            'provision.modeAdopt': '使用已有的 exe',
            'provision.downloadHint': '将从 GitHub Release（{repo}）下载，并校验 SHA256。',
            'provision.adoptHint': '填写本机已有的 dsh-multiver.exe 绝对路径：',
            'provision.doDownload': '下载并校验',
            'provision.doAdopt': '使用此文件',
            'provision.working': '处理中…',
            'provision.acquired': '已就绪的 dsh-multiver.exe：',
            'provision.checksumOk': 'SHA256 校验通过',
            'provision.adoptNoChecksum': '已有文件不做校验，请自行确认来源可信',
            'provision.finish': '设置工作目录并启动',
            'provision.reasonPrefix': '检测到：',
            'err.invalid-dir': '目录路径无效',
            'err.invalid-exe': 'exe 路径无效',
            'err.exe-missing': '找不到 dsh-multiver.exe',
            'err.dir-create-failed': '无法创建目录',
            'err.dir-readonly': '目录不可写',
            'err.download-failed': '下载失败',
            'err.http-error': '服务器返回错误',
            'err.checksum-mismatch': 'SHA256 校验失败，已拒绝该文件',
            'err.checksum-missing': '校验文件中没有该文件的记录',
            'err.rename-failed': '无法写入目标位置（文件可能正被占用）',
            'err.insecure-url': '仅支持 HTTPS 下载',
            'err.invalid-step': '未知的部署步骤',
            'err.write-failed': '写入文件失败',
            'update.title': 'multiver 更新',
            'update.current': '当前 {v}',
            'update.checking': '正在检查更新…',
            'update.working': '处理中…',
            'update.upToDate': '已是最新版本',
            'update.checkFailed': '检查失败：{e}',
            'update.recheck': '重新检查',
            'update.available': '有新版本 {v}',
            'update.changelog': '查看 Release',
            'update.doUpdate': '更新',
            'update.confirm': '更新会重启管理器，确认？',
            'update.yes': '确认',
            'update.no': '取消',
            'update.handedOff': '更新已就绪，管理器将退出以完成替换',
            'update.embeddedHint': '当前在内嵌窗口中。更新会中断本会话——建议在 multiver 主界面完成更新，或关闭该窗口后重试。',
            'err.embedded-blocked': '内嵌窗口下不能执行更新',
            'update.staged': '已暂存，multiver 下次启动时自动安装',
            'update.pendingInstall': '待安装：v{v}（multiver 下次启动时生效）',
            'update.workingWithTime': '处理中…（已用 {s}）',
            'update.noCancelHint': '更新期间不可取消：multiver 的 update 是阻塞调用，中途终止可能留下损坏的文件。',
            'err.download-url-missing': 'Release 资产缺失，无法下载新版本',
            'err.disk-full': '磁盘剩余空间不足',
            'err.aborted': '已取消',
            'err.all-sources-failed': '所有下载源均失败（检查网络或稍后重试）',
            'err.download-busy': '已有下载任务在进行中',
            'err.bad-binary': '下载内容不是可执行文件（可能是镜像返回了错误页）',
            'download.connecting': '正在连接…',
            'download.cancel': '取消下载',
            'download.cancelling': '正在取消…',
          },
          en: {
            section: 'Versions',
            title: 'dsh-multiver versions',
            loading: 'Loading version info…',
            retry: 'Retry',
            refresh: 'Refresh',
            defaultLabel: 'Default',
            runningLabel: 'Running',
            unknownVersion: 'unknown',
            pendingRestart: 'Set to {next}; currently running {running}. Restart dsh to take effect.',
            consistencyMismatch: 'The terminal dsh entry does not match the default version (shim may be broken). Re-run "set default" in multiver.',
            'col.version': 'Version',
            'col.installedAt': 'Installed',
            'col.flags': 'Flags',
            'badge.default': 'default',
            'badge.isolated': 'isolated',
            'badge.sharedHome': 'shared home',
            setDefault: 'Set default',
            switching: 'Setting…',
            confirmSwitch: 'Takes effect after restart. Confirm?',
            yes: 'Confirm',
            no: 'Cancel',
            switched: 'Default set to {version}. Restart dsh to take effect.',
            exePath: 'multiver: {exe}',
            detectedBy: 'via {by}',
            'conf.low': 'inferred',
            'conf.medium': 'from multiver',
            'conf.conflict': 'conflict',
            'conf.unknown': 'unknown',
            'err.exe-not-found': 'dsh-multiver.exe not found',
            'err.spawn-failed': 'Failed to launch dsh-multiver',
            'err.timeout': 'dsh-multiver timed out',
            'err.json-invalid': 'multiver too old, need ≥0.3.0',
            'err.cli-error': 'dsh-multiver returned an error',
            'err.version-not-installed': 'Version not installed',
            'err.invalid-version': 'Invalid version string',
            'err.unknown-endpoint': 'Unknown endpoint',
            'err.internal': 'Internal error',
            emptyTitle: 'multiver has no dsh versions yet',
            emptyHint: 'Install a version in dsh-multiver, then come back here.',
            emptyNote: 'multiver only manages the dsh copies it installed itself (under versions/ in the data root). A dsh installed via npm, pnpm, a Python wheel, or DeepSeek Harness Desktop is a separate installation — unaffected by this plugin and never listed here.',
            'err.network': 'Network or auth failure',
            'err.invalid-target': 'Unsupported target',
            'err.outside-root': 'Path is outside the data root',
            'err.open-failed': 'Failed to open',
            'err.launch-failed': 'Failed to launch multiver',
            launchMultiver: 'Open multiver',
            openMultiverDir: 'Open multiver folder',
            openMultiverSettings: 'Open multiver path settings',
            openVersionDir: 'Open version folder',
            copyDiagnostics: 'Copy diagnostics',
            copyFallbackHint: 'Clipboard unavailable. Copy the text below manually:',
            copyOk: 'Diagnostics copied to clipboard',
            conflictTitle: 'Launch source mismatch',
            conflictHint: 'The version multiver recorded differs from the one actually loaded. Restart dsh, or check the consistency details below.',
            consistencyDefaultEntry: 'Default version vs terminal entry',
            consistencyRoot: 'Data root vs exe location',
            consistencyExe: 'Actual exe source',
            consistencySep: ': ',
            consistencyBad: 'mismatch',
            consistencyUnknown: 'unknown',
            compatUnverified: 'This dsh ({running}) is not a verified version. The plugin was verified against {list}; later releases may break it. Please report issues on the multiver side.',
            'provision.title': 'First run: set up dsh-multiver',
            'provision.subtitle': 'Prepare dsh-multiver.exe and pick a working directory, then you can manage dsh versions here.',
            'provision.step1': 'Choose directory',
            'provision.step2': 'Get dsh-multiver.exe',
            'provision.step3': 'Finish and launch',
            'provision.pickDirHint': 'Where versions and config will live (created if missing; system directories are never used by default):',
            'provision.next': 'Next',
            'provision.back': 'Back',
            'provision.modeDownload': 'Download the latest',
            'provision.modeAdopt': 'Use an existing exe',
            'provision.downloadHint': 'Downloads from the GitHub Release ({repo}) and verifies SHA256.',
            'provision.adoptHint': 'Absolute path to an existing dsh-multiver.exe:',
            'provision.doDownload': 'Download and verify',
            'provision.doAdopt': 'Use this file',
            'provision.working': 'Working…',
            'provision.acquired': 'Ready:',
            'provision.checksumOk': 'SHA256 verified',
            'provision.adoptNoChecksum': 'Existing file is not verified — make sure the source is trustworthy',
            'provision.finish': 'Set directory and launch',
            'provision.reasonPrefix': 'Detected:',
            'err.invalid-dir': 'Invalid directory path',
            'err.invalid-exe': 'Invalid exe path',
            'err.exe-missing': 'dsh-multiver.exe not found',
            'err.dir-create-failed': 'Failed to create directory',
            'err.dir-readonly': 'Directory is not writable',
            'err.download-failed': 'Download failed',
            'err.http-error': 'Server returned an error',
            'err.checksum-mismatch': 'SHA256 mismatch — file rejected',
            'err.checksum-missing': 'Checksum file has no entry for this file',
            'err.rename-failed': 'Cannot write target (file may be in use)',
            'err.insecure-url': 'Only HTTPS downloads are allowed',
            'err.invalid-step': 'Unknown provisioning step',
            'err.write-failed': 'Failed to write file',
            'update.title': 'multiver update',
            'update.current': 'current {v}',
            'update.checking': 'Checking for updates…',
            'update.working': 'Working…',
            'update.upToDate': 'Already up to date',
            'update.checkFailed': 'Check failed: {e}',
            'update.recheck': 'Check again',
            'update.available': 'Version {v} available',
            'update.changelog': 'Release notes',
            'update.doUpdate': 'Update',
            'update.confirm': 'Updating restarts the manager. Continue?',
            'update.yes': 'Confirm',
            'update.no': 'Cancel',
            'update.handedOff': 'Update ready — the manager will exit to finish replacing itself',
            'update.embeddedHint': 'Running inside the embedded window. Updating interrupts this session — do it from the multiver main window, or close this window first.',
            'err.embedded-blocked': 'Update is not allowed from the embedded window',
            'update.staged': 'Staged — installs automatically on next multiver launch',
            'update.pendingInstall': 'Pending install: v{v} (applies on next multiver launch)',
            'update.workingWithTime': 'Working… ({s} elapsed)',
            'update.noCancelHint': 'Cannot be cancelled: multiver update is a blocking call and aborting it may leave a corrupted file.',
            'err.download-url-missing': 'Release asset missing, cannot download',
            'err.disk-full': 'Not enough free disk space',
            'err.aborted': 'Cancelled',
            'err.all-sources-failed': 'All download sources failed (check network or retry later)',
            'err.download-busy': 'Another download is in progress',
            'err.bad-binary': 'Downloaded content is not an executable (mirror may have returned an error page)',
            'download.connecting': 'Connecting…',
            'download.cancel': 'Cancel download',
            'download.cancelling': 'Cancelling…',
          },
        }), 'multiver-bridge: dictionaries')

        const Wrapped = (props) => h(LocaleCtx.Provider, { value: t }, h(VersionsPage, props))

        // 设置页导航图标：dsh 的 shell 按 id 白名单硬编码图标，第三方 section 一律拿到齿轮
        // （与「通用设置」重复）。slot 契约不提供 icon 字段，故按 label 文本定位我们的按钮，
        // 替换其内部 svg 内容。定位失败时静默放弃，仍显示原生齿轮，不影响功能。
        installNavIcon(() => t('section'))

        ctx.slots.inject('settings.section', () => {
          let dispose
          const register = () => {
            dispose?.()
            dispose = ctx.slots.register({
              name: 'settings.section',
              id: 'multiver-versions',
              order: 100,
              label: t('section'),
              locale: NS,
            }, Wrapped)
          }
          register()
          const offLocale = ctx.locale.subscribe(register)
          return () => { offLocale(); dispose?.() }
        })
      },
    }
  },
})
