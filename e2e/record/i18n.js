// 录屏用 i18n：直接复用前端词条，保证 getByRole 的名称与界面完全一致。
// REC_LANG 由 scripts/record-manual.ps1 注入；默认简体中文。
import zhCN from "../../src/composables/i18n/zh-CN.js";
import zhTW from "../../src/composables/i18n/zh-TW.js";
import enUS from "../../src/composables/i18n/en-US.js";
import jaJP from "../../src/composables/i18n/ja-JP.js";
import koKR from "../../src/composables/i18n/ko-KR.js";

const PACKS = {
  "zh-CN": zhCN,
  "zh-TW": zhTW,
  "en-US": enUS,
  "ja-JP": jaJP,
  "ko-KR": koKR,
};

export const REC_LANG = process.env.REC_LANG || "zh-CN";

export function T(key, params) {
  let s = PACKS[REC_LANG]?.[key];
  if (s === undefined) s = PACKS["zh-CN"]?.[key];
  if (s === undefined) return key;
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      s = s.replace(new RegExp("\\{" + k + "\\}", "g"), String(v));
    }
  }
  return s;
}
