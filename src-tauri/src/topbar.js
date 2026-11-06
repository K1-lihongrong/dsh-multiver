(function() {
  if (window.__dshTopbarInjected) return;
  window.__dshTopbarInjected = true;
  var VER = '__VER__';
  var URL = '__URL__';

  // ── 事件通道 ──────────────────────────────────────────────
  // 内嵌 dsh 窗口是 remote 上下文，应用自有命令（log_frontend / restart_version）
  // 被 ACL 默认拒绝；插件命令（plugin:event|emit，由 core:default 授权）可用。
  // 故所有通知统一走事件，由 Rust 侧接收处理。
  function emit(event, payload) {
    try {
      var it = window.__TAURI_INTERNALS__;
      if (it && typeof it.invoke === 'function') {
        it.invoke('plugin:event|emit', { event: event, payload: payload || {} }).catch(function(){});
      }
    } catch(_) {}
  }
  function log(msg) { emit('topbar-log', { msg: msg }); }
  function ctrl(action) { emit('topbar-ctrl', { action: action, version: VER }); }

  // ── 深浅色跟随 ────────────────────────────────────────────
  // 只看 body[data-ds-dark-theme] 属性；不要观察 html[style]
  // （会与 dsh 渲染抢时序导致闪退）。观察器 250ms 防抖。
  var isDark = false;
  function applyTheme() {
    var bar = document.getElementById('__dsh_topbar');
    if (!bar) return;
    // 直接设内联样式，不依赖 CSS 选择器 + !important 的覆盖能力。
    // 原因：实测 dsh 切深色后，[data-theme="dark"] 的 !important 规则没能覆盖
    // 初次绘制时写入的内联背景，导致标题栏停在浅色。内联赋值优先级最高且无歧义。
    if (isDark) {
      bar.style.background = '#1f2328';
      bar.style.borderBottomColor = '#333';
      bar.style.color = '#ddd';
    } else {
      bar.style.background = '#f7f7f8';
      bar.style.borderBottomColor = '#e5e5e5';
      bar.style.color = '#333';
    }
    emit('topbar-theme', { dark: isDark, version: VER });
  }
  function observeTheme() {
    if (!document.body) return;
    var timer = 0;
    function check() {
      var d = document.body.hasAttribute('data-ds-dark-theme');
      if (d !== isDark) { isDark = d; applyTheme(); }
    }
    check();
    new MutationObserver(function() {
      if (timer) clearTimeout(timer);
      timer = setTimeout(check, 250);
    }).observe(document.body, { attributes: true, attributeFilter: ['data-ds-dark-theme'] });
  }

  // ── 标题栏 ────────────────────────────────────────────────
  function inject() {
    if (!document.body) return setTimeout(inject, 50);
    if (document.getElementById('__dsh_topbar')) return;

    var H = 34;

    var bar = document.createElement('div');
    bar.id = '__dsh_topbar';
    bar.setAttribute('data-tauri-drag-region', '');
    bar.style.cssText = 'position:fixed;top:0;left:0;right:0;height:' + H + 'px;'
      + 'z-index:2147483647;display:flex;align-items:center;gap:6px;padding:0 8px;'
      + 'background:#f7f7f8;border-bottom:1px solid #e5e5e5;'
      + 'font:12px/1 system-ui,-apple-system,sans-serif;color:#333;user-select:none;'
      + 'transition:background .15s,border-color .15s,color .15s;';

    var label = document.createElement('span');
    label.textContent = 'DSH ' + VER;
    label.style.cssText = 'font-weight:600;white-space:nowrap;padding:0 4px;';
    bar.appendChild(label);

    // URL 默认折叠为小按钮，点击展开为可选中复制的输入框。
    // 原因：dsh 的 URL 很长（含 token），铺满中间区域会让标题栏几乎无处可拖拽。
    var urlWrap = document.createElement('div');
    // data-tauri-drag-region：Tauri 2 只认"被按下鼠标的元素自身"是否带该属性，
    // 不向上查找父元素。URL 折叠后中间大片空白由本元素占据，不加就无法拖动窗口。
    urlWrap.setAttribute('data-tauri-drag-region', '');
    urlWrap.style.cssText = 'flex:1;min-width:0;display:flex;align-items:center;';

    var urlPill = document.createElement('button');
    urlPill.textContent = '🔗';
    urlPill.title = '显示/复制 dsh 地址';
    urlPill.style.cssText = 'height:22px;padding:0 8px;display:inline-flex;align-items:center;'
      + 'cursor:pointer;background:transparent;border:1px solid transparent;border-radius:4px;'
      + 'font:12px/1 system-ui,sans-serif;color:inherit;opacity:.65;transition:opacity .12s,border-color .12s;';
    urlPill.onmouseenter = function(){ urlPill.style.opacity = '1'; urlPill.style.borderColor = 'rgba(127,127,127,.4)'; };
    urlPill.onmouseleave = function(){ urlPill.style.opacity = '.65'; urlPill.style.borderColor = 'transparent'; };

    var urlBox = document.createElement('input');
    urlBox.type = 'text';
    urlBox.value = URL;
    urlBox.readOnly = true;
    urlBox.title = '点击选中，Ctrl+C 复制';
    urlBox.style.cssText = 'flex:1;min-width:0;height:22px;background:transparent;'
      + 'border:1px solid #4f6ef7;border-radius:4px;padding:0 6px;'
      + 'font:11px/1 monospace;color:inherit;cursor:text;display:none;';
    urlBox.onclick = function(){ urlBox.select(); };

    function expandUrl() {
      urlPill.style.display = 'none';
      urlBox.style.display = 'block';
      urlBox.select();
    }
    function collapseUrl() {
      urlBox.style.display = 'none';
      urlPill.style.display = 'inline-flex';
    }
    urlPill.onclick = expandUrl;
    urlBox.onblur = collapseUrl;
    urlBox.onkeydown = function(e){ if (e.key === 'Escape') { urlBox.blur(); } };

    urlWrap.appendChild(urlPill);
    urlWrap.appendChild(urlBox);
    bar.appendChild(urlWrap);

    function mkBtn(icon, title, onClick, danger) {
      var b = document.createElement('button');
      b.textContent = icon;
      b.title = title;
      b.style.cssText = 'width:28px;height:22px;display:inline-flex;align-items:center;justify-content:center;'
        + 'cursor:pointer;background:transparent;border:0;border-radius:4px;'
        + 'font:14px/1 system-ui,sans-serif;color:inherit;padding:0;transition:background .12s,color .12s;';
      b.onmouseenter = function(){ b.style.background = danger ? '#e81123' : 'rgba(127,127,127,.15)'; if (danger) b.style.color = '#fff'; };
      b.onmouseleave = function(){ b.style.background = 'transparent'; if (danger) b.style.color = 'inherit'; };
      b.onclick = onClick;
      return b;
    }

    var btnRestart = mkBtn('⟳', '重启 dsh', function(){
      log('restart clicked');
      emit('topbar-restart', { version: VER });
    });
    bar.appendChild(btnRestart);

    var sep = document.createElement('span');
    sep.style.cssText = 'width:1px;height:16px;background:rgba(127,127,127,.25);margin:0 2px;';
    bar.appendChild(sep);

    bar.appendChild(mkBtn('—', '最小化', function(){ ctrl('minimize'); }));
    bar.appendChild(mkBtn('□', '最大化', function(){ ctrl('toggle-maximize'); }));
    bar.appendChild(mkBtn('×', '关闭', function(){ ctrl('close'); }, true));

    var style = document.createElement('style');
    style.textContent = '#__dsh_topbar button:focus-visible{outline:2px solid #4f6ef7;outline-offset:-2px;}';
    document.head.appendChild(style);

    document.body.appendChild(bar);

    document.body.style.paddingTop = H + 'px';
    document.body.style.boxSizing = 'border-box';

    observeTheme();
    log('injected ver=' + VER + ' dark=' + isDark);
  }

  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', inject);
  else inject();
})();
