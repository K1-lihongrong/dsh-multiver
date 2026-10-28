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
          h('div', null, t('emptyHint'))
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
            emptyTitle: '还没有已安装的版本',
            emptyHint: '请先在 dsh-multiver 里安装一个版本，再回到这里管理。',
            'err.network': '网络或鉴权失败',
            'err.invalid-target': '目标不受支持',
            'err.outside-root': '路径超出数据根',
            'err.open-failed': '打开失败',
            'err.launch-failed': '启动 multiver 失败',
            launchMultiver: '打开 multiver',
            openMultiverDir: '打开 multiver 目录',
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
            emptyTitle: 'No versions installed yet',
            emptyHint: 'Install a version in dsh-multiver first, then come back here.',
            'err.network': 'Network or auth failure',
            'err.invalid-target': 'Unsupported target',
            'err.outside-root': 'Path is outside the data root',
            'err.open-failed': 'Failed to open',
            'err.launch-failed': 'Failed to launch multiver',
            launchMultiver: 'Open multiver',
            openMultiverDir: 'Open multiver folder',
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
          },
        }), 'multiver-bridge: dictionaries')

        const Wrapped = (props) => h(LocaleCtx.Provider, { value: t }, h(VersionsPage, props))

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
