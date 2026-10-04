<script setup>
import { onMounted } from "vue";
import { notesCache as cache, hasNotes, setNotes } from "../composables/notesCache.js";

// 版本更新说明面板：自包含（自己拉取 dsh 上游 GitHub Releases 并消毒渲染）。
// 父组件只传 version，监听 close；打开时才加载（懒加载 + 内存缓存）。
const props = defineProps({
  version: { type: String, required: true },
  inline: { type: Boolean, default: false }, // 已安装版本用行内展开样式
});
const emit = defineEmits(["close", "open-url"]);

const RELEASE_REPO = "deepseek-ai/deepseek-harness";

function releaseUrl(v) {
  return "https://github.com/" + RELEASE_REPO + "/releases/tag/dsh-v" + v;
}

/// 消毒 GitHub release 的 HTML：只保留安全标签，去掉所有属性（a 仅保留安全 href）。
function sanitizeNotes(html) {
  const doc = new DOMParser().parseFromString(html, "text/html");
  const ALLOWED = new Set(["H1","H2","H3","H4","H5","H6","UL","OL","LI","P","BR",
    "STRONG","EM","B","I","CODE","PRE","BLOCKQUOTE","A","HR","TT","SPAN"]);
  // 这些标签连同其内容一起删除（不提升子内容，避免 script/style 内容泄漏）
  const DROP_WITH_CONTENT = new Set(["SCRIPT","STYLE","IFRAME","OBJECT","EMBED","LINK","META","NOSCRIPT","TEMPLATE"]);
  const walk = (node) => {
    for (const child of Array.from(node.childNodes)) {
      if (child.nodeType === 1) {
        if (DROP_WITH_CONTENT.has(child.tagName)) { child.remove(); continue; }
        if (!ALLOWED.has(child.tagName)) {
          const frag = doc.createDocumentFragment();
          while (child.firstChild) frag.appendChild(child.firstChild);
          node.replaceChild(frag, child);
          walk(node);
          return;
        }
        const href = child.tagName === "A" ? child.getAttribute("href") : null;
        for (const attr of Array.from(child.attributes)) child.removeAttribute(attr.name);
        if (child.tagName === "A" && href && /^(https?:\/\/|#)/i.test(href)) {
          child.setAttribute("href", href);
          if (!href.startsWith("#")) {
            child.setAttribute("target", "_blank");
            child.setAttribute("rel", "noopener noreferrer");
          }
        }
        walk(child);
      } else if (child.nodeType !== 3) {
        child.remove();
      }
    }
  };
  walk(doc.body);
  return doc.body.innerHTML;
}

/// 拉取某版本的更新说明（带缓存；失败静默降级）。
async function loadNotes(v) {
  // 命中模块级缓存（含"加载中"）则跳过：既避免重复请求，也做并发去重
  if (hasNotes(v)) return;
  setNotes(v, { status: "loading" });
  const url = releaseUrl(v);
  try {
    const api = "https://api.github.com/repos/" + RELEASE_REPO + "/releases/tags/dsh-v" + v;
    // html+json：让 GitHub 直接返回渲染好的 HTML（body_html），Markdown 已被转换
    const resp = await fetch(api, { headers: { Accept: "application/vnd.github.html+json" } });
    if (!resp.ok) throw new Error("HTTP " + resp.status);
    const data = await resp.json();
    const body = data && typeof data.body_html === "string" ? data.body_html : "";
    setNotes(v, { status: "ok", url, empty: !body.trim(), html: sanitizeNotes(body) });
  } catch (e) {
    setNotes(v, { status: "error", url });
  }
}

function openOriginal() {
  const entry = cache.value[props.version];
  emit("open-url", (entry && entry.url) || releaseUrl(props.version));
}

onMounted(() => loadNotes(props.version));
</script>

<template>
  <div class="notes-panel" :class="{ 'notes-inline': inline }">
    <div class="notes-head" @click="emit('close')" title="点击收起">
      <span class="notes-title">DSH {{ version }} 更新说明</span>
      <span class="notes-close">×</span>
    </div>
    <div class="notes-body">
      <div v-if="!cache[version] || cache[version].status === 'loading'" class="notes-hint">加载中...</div>
      <div v-else-if="cache[version].status === 'ok' && cache[version].empty" class="notes-hint">该版本没有提供更新说明。</div>
      <div v-else-if="cache[version].status === 'ok'" class="notes-html" v-html="cache[version].html"></div>
      <div v-else class="notes-hint">无法获取更新说明（可能网络不通）。可点击下方按钮在浏览器查看。</div>
    </div>
    <div class="notes-foot">
      <button class="btn small" @click="openOriginal">在浏览器打开原文</button>
    </div>
  </div>
</template>

<style scoped>
/* 就地展开的更新说明面板 */
.notes-panel {
  margin: 0 0 12px; padding: 10px 14px; border: 1px solid #e2e5ea; border-radius: 9px;
  background: #fafbfc;
}
.notes-inline { flex-basis: 100%; margin: 8px 0 0; }
/* 标题栏整行可点收起：加 padding 撑满、hover 有反馈，鼠标不必对准 × */
.notes-head {
  display: flex; align-items: center; justify-content: space-between;
  margin: -6px -8px 4px; padding: 6px 8px; border-radius: 6px;
  cursor: pointer; user-select: none; transition: background .12s;
}
.notes-head:hover { background: #eef1f5; }
.notes-title { font-weight: 600; font-size: 13px; color: #374151; }
.notes-close { font-size: 18px; line-height: 1; color: #9aa1ab; flex-shrink: 0; }
.notes-head:hover .notes-close { color: #d9534f; }
.notes-body { max-height: 260px; overflow-y: auto; }
.notes-hint { color: #9aa1ab; font-size: 12px; }
.notes-html { font-size: 13px; color: #374151; line-height: 1.65; }
.notes-html h1, .notes-html h2, .notes-html h3, .notes-html h4 {
  font-size: 13px; font-weight: 600; margin: 10px 0 4px; color: #1f2937;
}
.notes-html h3:first-child, .notes-html h4:first-child { margin-top: 0; }
.notes-html ul, .notes-html ol { margin: 4px 0; padding-left: 20px; }
.notes-html li { margin: 2px 0; }
.notes-html p { margin: 6px 0; }
.notes-html a { color: #4f6ef7; }
.notes-html code { background: #eef1f5; padding: 1px 5px; border-radius: 4px; font-size: 12px; }
.notes-html img { max-width: 100%; }
.notes-foot { margin-top: 8px; display: flex; justify-content: flex-end; }
</style>
