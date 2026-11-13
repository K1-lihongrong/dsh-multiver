// 批量转码：test-results 下的录屏 → docs/screenshots/manual 下的五语 WebP。
//
// 用法：node e2e/record/build-gifs.mjs --lang=zh|en|ja|ko|zh-TW [--gif] [--skip-gif]
//
// 每次录屏只对应一种界面语言（REC_LANG）；本脚本按 --lang 输出对应后缀：
//   zh → <key>.webp；en → <key>.en.webp；ja → .ja；ko → .ko；zh-TW → .zh-TW。
// 字幕缺失时用本地 Ollama 翻译并缓存到 %TEMP%/dsh-multiver/cues/i18n-cache.json。
//
// 关键坑：
// - -ss 0.8 裁掉启动白屏，ASS 里的时间轴要同步减 0.8。
// - subtitles 滤镜在 Windows 下路径要用正斜杠 + 转义冒号，故先复制到纯 ASCII 临时目录。
// - fps 从 10 降到 6：画面基本是静态界面 + 缓动光标，6fps 足够，体积降一半。
// - WebP 必须显式加 -loop 0，否则只播一次。
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, copyFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

const FFMPEG = process.env.FFMPEG || "D:/HRSoftstore/LLMchatl/ffmpeg-9.0-full_build/bin/ffmpeg.exe";
const ROOT = process.cwd();
const SRC = join(ROOT, "test-results");
const DEST = join(ROOT, "docs", "screenshots", "manual");
const TMP = join(tmpdir(), "dsh-multiver", "rec");
const CUE_DIR = join(tmpdir(), "dsh-multiver", "cues");
const CUE_CACHE = join(CUE_DIR, "i18n-cache.json");

const TRIM = 0.8;
const FPS = 6;
const WIDTH = 960;
const QUALITY = 75;
const SUBTITLE_BAND = 44;
const FORMAT = process.argv.includes("--gif") ? "gif" : "webp";
const EXT = "." + FORMAT;

const LANGS = {
  zh: { suffix: "", ui: "zh-CN", font: "Microsoft YaHei" },
  en: { suffix: ".en", ui: "en-US", font: "Segoe UI" },
  ja: { suffix: ".ja", ui: "ja-JP", font: "Yu Gothic" },
  ko: { suffix: ".ko", ui: "ko-KR", font: "Malgun Gothic" },
  "zh-TW": { suffix: ".zh-TW", ui: "zh-TW", font: "Microsoft JhengHei" },
};

const langArg = (process.argv.find((a) => a.startsWith("--lang=")) || "--lang=zh").split("=")[1];
if (langArg === "all" || !LANGS[langArg]) {
  console.error("--lang 只接受 zh、en、ja、ko、zh-TW；界面语言必须与本次录屏的 REC_LANG 一致。");
  process.exit(2);
}
const LANG = LANGS[langArg];
if (process.env.REC_LANG && process.env.REC_LANG !== LANG.ui) {
  console.error("REC_LANG=" + process.env.REC_LANG + " 与 --lang=" + langArg + "（应为 " + LANG.ui + "）不一致。");
  process.exit(2);
}

const MAP = {
  "01-main": "01-main",
  "02-env-fail": "02-env-fail",
  "03-installed": "03-installed",
  "04-batch": "04-batch",
  "05-remote": "05-remote",
  "06-progress": "06-progress",
  "07-modal-net": "07-modal-net",
  "08-modal-priv": "08-modal-priv",
  "09-isolated": "09-isolated",
  "10-topbar-dot": "10-topbar-dot",
  "11-maintenance": "11-maintenance",
  "12-paths": "12-paths",
  "13-plugin-installed": "13-plugin-installed",
  "14-plugin-catalog": "14-plugin-catalog",
  "15-appearance": "15-appearance",
  "17-instance-panel": "17-instance-panel",
  "18-plugin-add": "18-plugin-add",
};

const SKIP = process.argv.includes("--skip-gif");

function assTime(sec) {
  const s = Math.max(0, sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = Math.floor(s % 60);
  const cs = Math.round((s - Math.floor(s)) * 100);
  const p = (n, w = 2) => String(n).padStart(w, "0");
  return h + ":" + p(m) + ":" + p(ss) + "." + p(cs);
}

function buildAss(cues, lang, duration) {
  const lines = [];
  lines.push("[Script Info]");
  lines.push("ScriptType: v4.00+");
  lines.push("PlayResX: " + WIDTH);
  lines.push("PlayResY: " + (600 + SUBTITLE_BAND));
  lines.push("WrapStyle: 0");
  lines.push("ScaledBorderAndShadow: yes");
  lines.push("");
  lines.push("[V4+ Styles]");
  lines.push(
    "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, " +
      "Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, " +
      "Alignment, MarginL, MarginR, MarginV, Encoding",
  );
  lines.push(
    "Style: Default," + LANGS[lang].font + ",22,&H00FFFFFF,&H000000FF,&H00000000,&H00000000," +
      "1,0,0,0,100,100,0,0,1,2,0,2,24,24,12,1",
  );
  lines.push("");
  lines.push("[Events]");
  lines.push("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text");

  for (let i = 0; i < cues.length; i++) {
    const c = cues[i];
    const text = c[lang];
    if (!text) throw new Error("字幕缺失：" + lang + " / " + (c.zh || "").slice(0, 20));
    const start = Math.max(0, c.t - TRIM);
    const next = cues[i + 1];
    const end = next ? Math.max(start + 0.4, next.t - TRIM) : Math.max(start + 0.4, duration - TRIM);
    if (end <= 0) continue;
    lines.push("Dialogue: 0," + assTime(start) + "," + assTime(end) + ",Default,,0,0,0,," + text);
  }
  return lines.join("\n") + "\n";
}

function loadCache() {
  try { return JSON.parse(readFileSync(CUE_CACHE, "utf8")); } catch (_) { return {}; }
}
const cache = loadCache();
function saveCache() {
  mkdirSync(CUE_DIR, { recursive: true });
  writeFileSync(CUE_CACHE, JSON.stringify(cache, null, 2), "utf8");
}

/** 单条字幕翻译：只在缺语言时调用，结果写入缓存。 */
async function translateCaption(text, lang) {
  const key = lang + "|" + text;
  if (cache[key]) return cache[key];
  const body = {
    model: "qwen3.5:9b",
    stream: false,
    think: false,
    messages: [
      { role: "system", content: "You translate one short UI caption. Preserve product names, paths, commands and punctuation. Output only the translation, no quotes." },
      { role: "user", content: "Translate to " + lang + ": " + text },
    ],
    options: { num_ctx: 2048, num_predict: 256, temperature: 0.2 },
  };
  const res = await fetch("http://127.0.0.1:11434/api/chat", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error("Ollama HTTP " + res.status);
  const json = await res.json();
  const out = String(json.message?.content || "").trim().replace(/^["'“”]+|["'“”]+$/g, "");
  if (!out) throw new Error("Ollama 返回空字幕：" + text.slice(0, 20));
  cache[key] = out;
  saveCache();
  return out;
}

async function resolveCues(cues, target) {
  const out = [];
  for (const c of cues) {
    const row = { t: c.t, zh: c.zh };
    if (target !== "zh") {
      row[target] = c[target] || await translateCaption(c.zh, target);
    }
    out.push(row);
  }
  return out;
}

async function main() {
  mkdirSync(DEST, { recursive: true });
  mkdirSync(TMP, { recursive: true });

  const dirs = readdirSync(SRC, { withFileTypes: true }).filter((d) => d.isDirectory());
  let ok = 0;
  let fail = 0;

  for (const dir of dirs) {
    const key = Object.keys(MAP).find((k) => dir.name.startsWith(k));
    if (!key) continue;
    const src = join(SRC, dir.name, "video.webm");
    if (!existsSync(src)) {
      console.log("跳过（无视频）：" + dir.name);
      continue;
    }
    if (SKIP) {
      console.log("[dry] " + src + " -> " + MAP[key] + LANG.suffix);
      continue;
    }

    const tmpWebm = join(TMP, key + ".webm");
    copyFileSync(src, tmpWebm);

    const cuePath = join(CUE_DIR, key + ".json");
    const rawCues = existsSync(cuePath) ? JSON.parse(readFileSync(cuePath, "utf8")) : null;
    const cues = rawCues && rawCues.length ? await resolveCues(rawCues, langArg) : null;

    let duration = 30;
    const probe = spawnSync(FFMPEG.replace(/ffmpeg(\.exe)?$/i, "ffprobe$1"), [
      "-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", tmpWebm,
    ], { encoding: "utf8" });
    if (probe.status === 0 && probe.stdout.trim()) duration = parseFloat(probe.stdout.trim());

    const out = join(DEST, MAP[key] + LANG.suffix + EXT);
    const tmpOut = join(TMP, key + LANG.suffix + EXT);

    const vfParts = [
      "fps=" + FPS,
      "scale=" + WIDTH + ":-2:flags=lanczos",
      "pad=iw:ih+" + SUBTITLE_BAND + ":0:0:color=black",
    ];
    if (cues && cues.length) {
      const assName = "sub_" + key + "_" + langArg + ".ass";
      writeFileSync(join(TMP, assName), buildAss(cues, langArg, duration), "utf8");
      vfParts.push("subtitles=" + assName);
    }

    const codecArgs = FORMAT === "webp"
      ? ["-c:v", "libwebp_anim", "-lossless", "0", "-quality", String(QUALITY)]
      : [];

    const args = [
      "-y", "-ss", String(TRIM), "-i", tmpWebm,
      "-vf", vfParts.join(","), "-loop", "0",
      ...codecArgs, tmpOut,
    ];
    const r = spawnSync(FFMPEG, args, { cwd: TMP, encoding: "utf8" });
    if (r.status !== 0) {
      console.log("失败：" + MAP[key] + LANG.suffix);
      console.log((r.stderr || "").slice(-500));
      fail++;
      continue;
    }
    copyFileSync(tmpOut, out);
    ok++;
    console.log("已生成 " + MAP[key] + LANG.suffix + (cues && cues.length ? "（字幕 " + cues.length + " 条）" : "（无字幕）"));
  }
  console.log("\n完成：" + ok + " 个文件，失败：" + fail + " 个");
  if (fail) process.exit(1);
}

await main();
