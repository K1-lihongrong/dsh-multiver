#!/usr/bin/env node
// 以 zh-CN 为基准，检查其他语言文件的 key 是否齐全（A4，v0.3.7）。
//
// 为什么需要它：5 个语言文件各自独立维护，改文案时可能只改一种语言，
// 导致 key 缺失——运行时静默回退中文，编译与构建都不报错。
// 这条脚本把「哪个语言少了哪个 key」提前到 CI。
//
// 用法：node scripts/check-i18n.mjs
// 退出码：0 = 全部一致；1 = 存在缺失或多余。
import { resolve, dirname } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const base = resolve(__dirname, "../src/composables/i18n");

const LANGS = ["zh-CN", "zh-TW", "en-US", "ja-JP", "ko-KR"];
const REF = "zh-CN";

// 词条表是扁平的「点分 key → 字符串」，但保留嵌套对象处理以兼容未来结构。
function flatten(obj, prefix = "") {
  const out = [];
  for (const k of Object.keys(obj)) {
    const v = obj[k];
    if (typeof v === "object" && v !== null && !Array.isArray(v)) {
      out.push(...flatten(v, prefix + k + "."));
    } else {
      out.push(prefix + k);
    }
  }
  return out;
}

async function loadKeys(lang) {
  const mod = await import(pathToFileURL(resolve(base, lang + ".js")).href);
  return new Set(flatten(mod.default));
}

const refKeys = await loadKeys(REF);
console.log(`${REF}: ${refKeys.size} keys`);

let failed = false;
for (const lang of LANGS) {
  if (lang === REF) continue;
  const keys = await loadKeys(lang);
  const missing = [...refKeys].filter((k) => !keys.has(k));
  const extra = [...keys].filter((k) => !refKeys.has(k));
  if (missing.length || extra.length) {
    failed = true;
    console.error(`\n✗ ${lang}: 缺 ${missing.length} 个，多 ${extra.length} 个`);
    if (missing.length)
      console.error("  缺失: " + missing.slice(0, 10).join(", ") + (missing.length > 10 ? " ..." : ""));
    if (extra.length)
      console.error("  多余: " + extra.slice(0, 10).join(", ") + (extra.length > 10 ? " ..." : ""));
  } else {
    console.log(`✓ ${lang}: ${keys.size} keys`);
  }
}

if (failed) process.exit(1);
