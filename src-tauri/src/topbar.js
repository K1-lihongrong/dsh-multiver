(function() {
  if (window.__dshDotInjected) return;
  window.__dshDotInjected = true;
  var VER = '__VER__';
  var URL = '__URL__';
  function inject() {
    if (!document.body) return setTimeout(inject, 50);
    if (document.getElementById('__dsh_dot')) return;
    var open = false;
    var dot = document.createElement('div');
    dot.id = '__dsh_dot';
    var panel = document.createElement('div');
    panel.id = '__dsh_panel';
    panel.style.cssText = 'position:fixed;top:10px;left:10px;z-index:2147483647;display:none;background:#1f2328;color:#fff;font:12px/1.6 system-ui,sans-serif;padding:10px 12px;border-radius:8px;box-shadow:0 4px 16px rgba(0,0,0,.4);min-width:300px;max-width:70vw;';
    function setMode(mode) {
      if (mode === 'open') {
        dot.style.cssText = 'position:fixed;top:10px;left:10px;z-index:2147483647;width:26px;height:26px;border-radius:8px;background:#4f6ef7;cursor:pointer;box-shadow:0 1px 4px rgba(0,0,0,.25);opacity:1;display:flex;align-items:center;justify-content:center;color:#fff;font:11px/1 system-ui,sans-serif;';
        dot.textContent = '';
      } else if (mode === 'pill') {
        dot.style.cssText = 'position:fixed;top:10px;left:10px;z-index:2147483647;height:20px;padding:0 8px;border-radius:10px;background:#4f6ef7;cursor:pointer;box-shadow:0 1px 4px rgba(0,0,0,.25);opacity:1;display:flex;align-items:center;justify-content:center;color:#fff;font:11px/1 system-ui,sans-serif;white-space:nowrap;';
        dot.textContent = VER;
      } else {
        dot.style.cssText = 'position:fixed;top:10px;left:10px;z-index:2147483647;width:8px;height:8px;border-radius:50%;background:#4f6ef7;cursor:pointer;transition:all .18s cubic-bezier(.4,0,.2,1);box-shadow:0 1px 4px rgba(0,0,0,.2);opacity:.55;';
        dot.textContent = '';
      }
    }
    setMode('dot');
    var head = document.createElement('div');
    head.style.cssText = 'display:flex;align-items:center;gap:8px;margin-bottom:8px;';
    var title = document.createElement('span');
    title.textContent = 'DSH ' + VER;
    title.style.cssText = 'font-weight:600;white-space:nowrap;flex:1;';
    var btnClose = document.createElement('button');
    btnClose.textContent = '\u00d7';
    btnClose.title = '收起';
    btnClose.style.cssText = 'cursor:pointer;background:transparent;color:#fff;border:0;font-size:16px;line-height:1;padding:0 4px;';
    head.appendChild(title); head.appendChild(btnClose);
    var urlBox = document.createElement('input');
    urlBox.type = 'text'; urlBox.value = URL; urlBox.readOnly = true;
    urlBox.title = '点击可选中，复制到浏览器打开';
    urlBox.style.cssText = 'width:100%;box-sizing:border-box;background:#2b2f36;color:#cfd8e3;border:1px solid #3a3f47;border-radius:4px;padding:3px 8px;font:12px/1.6 monospace;margin-bottom:8px;';
    urlBox.onclick = function(){ urlBox.select(); };
    var btnRestart = document.createElement('button');
    btnRestart.textContent = '重启';
    btnRestart.style.cssText = 'cursor:pointer;background:#4f6ef7;color:#fff;border:0;border-radius:4px;padding:4px 12px;font-size:12px;';
    btnRestart.onclick = function(){
      try {
        var inv = window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke;
        if (inv) { inv('restart_version', { version: VER }).catch(function(){ location.reload(); }); return; }
      } catch(e) {}
      location.reload();
    };
    function show(){ open=true; panel.style.display='block'; setMode('open'); }
    function hide(){ open=false; panel.style.display='none'; setMode('dot'); }
    dot.onmouseenter = function(){ if(!open) setMode('pill'); };
    dot.onmouseleave = function(){ if(!open) setMode('dot'); };
    dot.onclick = function(){ open ? hide() : show(); };
    btnClose.onclick = hide;
    document.addEventListener('keydown', function(e){ if(e.key==='Escape' && open) hide(); });
    panel.appendChild(head); panel.appendChild(urlBox); panel.appendChild(btnRestart);
    document.body.appendChild(dot);
    document.body.appendChild(panel);
  }
  if (document.readyState === 'loading') document.addEventListener('DOMContentLoaded', inject);
  else inject();
})();
