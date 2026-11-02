<script setup>
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { t } from "../composables/useI18n.js";
import { useToast } from "../composables/useToast.js";

// 「推荐」标签页：分类标签栏、双行卡片、描述浮窗、星标排序。
// 从 DsPluginPanel 拆出（v0.3.3 D21）。数据源与交互都在本组件内，
// 只有「已安装哪些包」来自壳（installedNames），用于卡片上的「已安装」标记。
const props = defineProps({
  active: { type: Boolean, default: false },
  installedNames: { type: Object, required: true },
  installing: { type: Boolean, default: false },
  busy: { type: Boolean, default: false },
});
const emit = defineEmits(["install"]);

const { notify } = useToast();

const catalog = ref(null); // { plugins, source, stale, failure }
const catalogLoading = ref(false);
const catalogError = ref("");
/// 后台检查远端更新的进行中标志，与 catalogLoading 分开——本地列表已渲染，只是顶部显示「正在检查」
const catalogChecking = ref(false);
const catalogCheckError = ref("");

/// 读本地列表（缓存 → 内置）。不联网，立即返回。
async function loadCatalogLocal() {
  if (catalogLoading.value) return;
  catalogLoading.value = true;
  catalogError.value = "";
  try {
    catalog.value = await invoke("ds_plugin_catalog");
  } catch (e) {
    catalogError.value = "" + e;
  } finally {
    catalogLoading.value = false;
  }
}

/// 检查远端更新。updated=true 时重新读本地并渲染新内容。
async function checkCatalogRemote(force) {
  if (catalogChecking.value) return;
  catalogChecking.value = true;
  catalogCheckError.value = "";
  try {
    const r = await invoke("ds_plugin_catalog_check", { force: !!force });
    if (r?.updated) {
      await loadCatalogLocal();
    } else if (r?.error) {
      catalogCheckError.value = r.error;
    }
  } catch (e) {
    catalogCheckError.value = "" + e;
  } finally {
    catalogChecking.value = false;
  }
}

/// 进入推荐页：先渲染本地，再后台检查更新。壳切标签时调用。
function activate() {
  if (!catalog.value && !catalogLoading.value) {
    loadCatalogLocal().then(() => checkCatalogRemote(false));
  } else {
    checkCatalogRemote(false);
  }
}

/// 当前选中的分类标签。"__all__" 表示「全部」。
const activeCatalogCat = ref("__all__");

/// 分类列表（按首次出现顺序去重），首项是「全部」。
const catalogCategories = computed(() => {
  const list = catalog.value?.plugins || [];
  const seen = [];
  for (const p of list) {
    const c = (p.category || "").trim();
    if (c && !seen.includes(c)) seen.push(c);
  }
  return [{ key: "__all__", label: t("dsplugins.catalogAll") }].concat(
    seen.map((c) => ({ key: c, label: c })),
  );
});

/// 当前标签下要展示的条目。
///
/// 置顶：tags 含 `featured`（推荐）或 `same-author`（与本软件同作者）。
/// 纯排序，不改变分类归属。
/// 不再按分类分组：标签栏已承担导航，分组会产生多条参差边缘。
const catalogItems = computed(() => {
  const list = catalog.value?.plugins || [];
  const filtered =
    activeCatalogCat.value === "__all__"
      ? list
      : list.filter((p) => (p.category || "").trim() === activeCatalogCat.value);
  // 稳定排序：星标在前，其余保持目录顺序
  return filtered
    .map((p, i) => {
      const tags = p.tags || [];
      const starred = tags.includes("featured") || tags.includes("same-author");
      return { p, i, star: starred ? 1 : 0 };
    })
    .sort((a, b) => b.star - a.star || a.i - b.i)
    .map((x) => x.p);
});

/// 该条目是否显示星标。
function isStarred(p) {
  const tags = p.tags || [];
  return tags.includes("featured") || tags.includes("same-author");
}

/// 星标的悬停说明：同作者优先，其次是推荐。
function starTitle(p) {
  const tags = p.tags || [];
  if (tags.includes("same-author")) return t("dsplugins.catalogSameAuthor");
  return t("dsplugins.catalogFeatured");
}

/// 分类标签栏是否溢出（决定是否显示 ‹ › 控件）。
const catalogTabsRef = ref(null);
const catalogTabsOverflow = ref(false);
const catalogTabsAtEnd = ref(false);

function updateCatalogTabsState() {
  const el = catalogTabsRef.value;
  if (!el) return;
  catalogTabsOverflow.value = el.scrollWidth > el.clientWidth + 1;
  catalogTabsAtEnd.value = el.scrollLeft + el.clientWidth >= el.scrollWidth - 1;
}

function scrollCatalogTabs(dir) {
  const el = catalogTabsRef.value;
  if (!el) return;
  el.scrollBy({ left: dir * 120, behavior: "smooth" });
}

/// 打开外部链接。Tauri WebView 里 <a target="_blank"> 不生效，必须走 open_url 命令。
async function openExternal(url) {
  if (!url) return;
  try {
    await invoke("open_url", { url });
  } catch (e) {
    notify(String(e));
  }
}

/// 推荐项的安装 spec：有 spec 用 spec，否则用包名。
const catalogSpec = (p) => p.spec || p.name;

/// 推荐项的 npmjs 链接。noNpm 为 true 时不生成（包不在 npm 上，链接会 404）。
const npmUrl = (p) =>
  p.noNpm ? "" : "https://www.npmjs.com/package/" + encodeURIComponent(p.name).replace(/%2F/g, "/");

/// 从推荐页发起安装：把 spec 交给壳，由壳打开安装对话框并预填。
function installFromCatalog(p) {
  emit("install", catalogSpec(p));
}

// ---------- 推荐卡片：描述浮窗 ----------
//
// 卡片第二行只放一行描述，溢出时右侧渐隐。完整描述在浮窗里。
// 触发方式三种：悬停 400ms、点击卡片、点「查看完整描述」按钮。
// 不挂在 dsh 的 settings.section 之外——浮窗是组件内部的固定定位元素。
const catalogTip = ref(null); // { text, left, top, width }
let catalogHoverTimer = null;
let catalogHideTimer = null;

function showCatalogTip(p, el) {
  if (!el) return;
  const rect = el.getBoundingClientRect();
  // 右边缘对齐卡片右边缘；卡片窄于 320px 时收窄到卡片宽度
  const width = Math.min(320, rect.width);
  catalogTip.value = {
    text: p.description || "",
    left: rect.right - width,
    top: rect.bottom + 4,
    width,
  };
}

function hideCatalogTip() {
  clearTimeout(catalogHoverTimer);
  clearTimeout(catalogHideTimer);
  catalogTip.value = null;
}

/// 卡片移出：延迟关闭，给鼠标移到浮窗的时间。进入浮窗会取消这次关闭。
function scheduleHideCatalogTip() {
  clearTimeout(catalogHideTimer);
  catalogHideTimer = setTimeout(() => {
    catalogTip.value = null;
  }, 200);
}

/// 鼠标进入浮窗：取消待执行的关闭。
function cancelHideCatalogTip() {
  clearTimeout(catalogHideTimer);
}

function onCardEnter(p, e) {
  clearTimeout(catalogHoverTimer);
  clearTimeout(catalogHideTimer);
  const el = e.currentTarget.closest?.(".dp-catalog-card") || e.currentTarget;
  catalogHoverTimer = setTimeout(() => showCatalogTip(p, el), 400);
}

/// 卡片移出：按鼠标的真实去向决定。
///
/// - 移向浮窗 → 保留（由浮窗自己的 mouseleave 负责关闭）
/// - 移向其他元素 → 立即关闭（不再等 200ms，否则会显得热区比卡片大一圈）
/// - relatedTarget 为 null（落在卡片与浮窗之间几像素的间隙）→ 给 80ms 缓冲
function onCardLeave(e) {
  clearTimeout(catalogHoverTimer);
  clearTimeout(catalogHideTimer);
  const to = e && e.relatedTarget;
  if (to instanceof Element && to.closest(".dp-cc-tip")) return;
  if (to === null) {
    catalogHideTimer = setTimeout(() => {
      catalogTip.value = null;
    }, 80);
    return;
  }
  catalogTip.value = null;
}

// 浮窗：点外部 / 窗口缩放时关闭
function onDocClickCloseTip(e) {
  if (!catalogTip.value) return;
  if (e.target.closest?.(".dp-catalog-card") || e.target.closest?.(".dp-cc-tip")) return;
  hideCatalogTip();
}
function onResizeCloseTip() {
  if (catalogTip.value) hideCatalogTip();
}

onMounted(() => {
  document.addEventListener("click", onDocClickCloseTip);
  window.addEventListener("resize", onResizeCloseTip);
});

onUnmounted(() => {
  clearTimeout(catalogHoverTimer);
  clearTimeout(catalogHideTimer);
  document.removeEventListener("click", onDocClickCloseTip);
  window.removeEventListener("resize", onResizeCloseTip);
});

// 列表渲染完再判一次标签栏是否溢出
watch(catalog, () => {
  setTimeout(updateCatalogTabsState, 0);
}, { flush: "post" });

// 切到推荐标签时激活（首次拉本地列表 + 后台检查远端）
watch(() => props.active, (on) => {
  if (on) activate();
}, { immediate: true });
</script>

<template>
  <div>
    <!-- 远端检查状态：本地内容已渲染，这里只是附加提示 -->
    <div v-if="catalogChecking" class="dp-catalog-stale dp-catalog-checking">
      <span class="dp-spinner"></span>
      {{ t("dsplugins.catalogChecking") }}
    </div>
    <div v-else-if="catalogCheckError" class="dp-catalog-stale">
      {{ t("dsplugins.catalogStale") }}
      <button class="dp-link-btn" @click="checkCatalogRemote(true)" :disabled="catalogChecking">
        {{ t("dsplugins.catalogRetry") }}
      </button>
    </div>
    <div v-if="catalogLoading" class="dp-state">
      <span class="dp-spinner"></span>
      <span>{{ t("dsplugins.loading") }}</span>
    </div>
    <div v-else-if="catalogError" class="dp-state dp-state-empty">
      <span>{{ catalogError }}</span>
      <button class="dp-link-btn" @click="loadCatalogLocal()">{{ t("dsplugins.catalogRetry") }}</button>
    </div>
    <div v-else-if="!catalogItems.length" class="dp-state dp-state-empty">
      <span class="dp-state-icon">🧩</span>
      <span>{{ t("dsplugins.catalogEmpty") }}</span>
    </div>
    <template v-else>
      <!-- 分类标签栏：分类多时溢出，右侧出现 ‹ › 滚动控件 -->
      <div class="dp-cat-tabs">
        <div
          ref="catalogTabsRef"
          class="dp-cat-tabs-scroll"
          @scroll="updateCatalogTabsState"
        >
          <button
            v-for="c in catalogCategories"
            :key="c.key"
            class="dp-cat-tab"
            :class="{ on: activeCatalogCat === c.key }"
            @click="activeCatalogCat = c.key"
          >{{ c.label }}</button>
        </div>
        <div v-if="catalogTabsOverflow" class="dp-cat-nav">
          <button title="向前" @click="scrollCatalogTabs(-1)">‹</button>
          <button title="向后" :disabled="catalogTabsAtEnd" @click="scrollCatalogTabs(1)">›</button>
        </div>
        <button
          class="dp-cat-refresh"
          :title="t('dsplugins.catalogRefresh')"
          :disabled="catalogChecking"
          @click="checkCatalogRemote(true)"
        >↻</button>
      </div>

      <!-- 单网格：星标排最前，不再按分类分组 -->
      <ul class="dp-list">
        <li
          v-for="p in catalogItems"
          :key="p.name"
          class="dp-catalog-card"
        >
          <!-- 第一行：图标 + ⭐ + 包名 + 徽标 + 安装 -->
          <div class="dp-cc-row1">
            <span class="dp-cc-icon">🧩</span>
            <span
              v-if="isStarred(p)"
              class="dp-cc-star"
              :title="starTitle(p)"
            >⭐</span>
            <code class="dp-cc-name" :title="p.name">{{ p.name }}</code>
            <span v-if="installedNames.has(p.name)" class="dp-badge dp-badge-bundle">
              {{ t("dsplugins.catalogInstalled") }}
            </span>
            <span v-if="p.minDshVersion" class="dp-badge">
              {{ t("dsplugins.catalogMinDsh", { v: p.minDshVersion }) }}
            </span>
            <button
              class="dp-btn dp-btn-primary dp-btn-sm dp-cc-install"
              :disabled="busy || installing || installedNames.has(p.name)"
              @click.stop="installFromCatalog(p)"
            >{{ t("dsplugins.catalogInstall") }}</button>
          </div>

          <!-- 第二行：链接 + 单行描述（溢出渐隐）+ 查看完整描述 -->
          <div class="dp-cc-row2">
            <span v-if="p.repo || npmUrl(p)" class="dp-cc-links">
              <button
                v-if="p.repo"
                class="dp-cc-link"
                :title="t('dsplugins.catalogRepo')"
                @click.stop="openExternal(p.repo)"
              >GitHub ↗</button>
              <button
                v-if="npmUrl(p)"
                class="dp-cc-link"
                :title="t('dsplugins.catalogNpm')"
                @click.stop="openExternal(npmUrl(p))"
              >npmjs ↗</button>
            </span>
            <span
              class="dp-cc-descwrap"
              @mouseenter="onCardEnter(p, $event)"
              @mouseleave="onCardLeave"
            >
              <span class="dp-cc-desc">{{ p.description }}</span>
              <button
                class="dp-cc-more"
                @click.stop="showCatalogTip(p, $event.currentTarget.closest('.dp-catalog-card'))"
              >{{ t("dsplugins.catalogViewDesc") }}</button>
            </span>
          </div>
        </li>
      </ul>
    </template>

    <!-- 描述浮窗：fixed 定位，右边缘对齐卡片右边缘 -->
    <Transition name="dp-tip">
      <div
        v-if="catalogTip"
        class="dp-cc-tip"
        :style="{ left: catalogTip.left + 'px', top: catalogTip.top + 'px', width: catalogTip.width + 'px' }"
        @mouseenter="cancelHideCatalogTip"
        @mouseleave="hideCatalogTip"
        @click.stop
      >{{ catalogTip.text }}</div>
    </Transition>
  </div>
</template>
