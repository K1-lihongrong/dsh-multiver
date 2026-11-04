// 批量转码：test-results 下的录屏 → docs/screenshots/manual 下的 GIF（含多语字幕）。
//
// 用法：node e2e/record/build-gifs.mjs [--skip-gif] [--lang=zh|en|ja|all]
//
// 三语字幕：spec 里的 cue(zh, en, ja) 写入 %TEMP%/dsh-multiver/cues/<key>.json，
// 本脚本读出来生成三份 ASS，分别烧进三份 GIF：
//   <key>.gif      中文（手册引用的是这份）
//   <key>.en.gif   英文
//   <key>.ja.gif   日文
//
// 关键坑：
// - -ss 0.8 裁掉启动白屏，ASS 里的时间轴要同步减 0.8。
// - subtitles 滤镜在 Windows 下路径要用正斜杠 + 转义冒号，故先复制到纯 ASCII 临时目录。
// - fps 从 10 降到 6：画面基本是静态界面 + 缓动光标，6fps 足够，体积降一半。
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

/** 裁掉的起始秒数（启动白屏 + 启动页） */
const TRIM = 0.8;
/** 输出帧率 */
const FPS = 6;
/** 输出宽度（高度按比例） */
const WIDTH = 960;

const LANG_ARG = (process.argv.find((a) => a.startsWith("--lang=")) || "--lang=all").split("=")[1];
const LANGS = LANG_ARG === "all" ? ["zh", "en", "ja"] : [LANG_ARG];

// spec 前缀 → 输出文件名
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
};

// 各语言字体：中 → 微软雅黑，英 → Segoe UI，日 → 游ゴシック
const FONT = { zh: "Microsoft YaHei", en: "Segoe UI", ja: "Yu Gothic" };

/** 秒 → ASS 时间戳 H:MM:SS.cc */
function assTime(sec) {
  const s = Math.max(0, sec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const ss = Math.floor(s % 60);
  const cs = Math.round((s - Math.floor(s)) * 100);
  const p = (n, w = 2) => String(n).padStart(w, "0");
  return h + ":" + p(m) + ":" + p(ss) + "." + p(cs);
}

/** 由 cues 生成 ASS 内容（单语言）。 */
function buildAss(cues, lang, duration) {
  const lines = [];
  lines.push("[Script Info]");
  lines.push("ScriptType: v4.00+");
  lines.push("PlayResX: " + WIDTH);
  lines.push("PlayResY: 600");
  lines.push("WrapStyle: 0");
  lines.push("ScaledBorderAndShadow: yes");
  lines.push("");
  lines.push("[V4+ Styles]");
  lines.push(
    "Format: Name, Fontname, Fontsize, PrimaryColour, SecondaryColour, OutlineColour, BackColour, " +
      "Bold, Italic, Underline, StrikeOut, ScaleX, ScaleY, Spacing, Angle, BorderStyle, Outline, Shadow, " +
      "Alignment, MarginL, MarginR, MarginV, Encoding",
  );
  // BorderStyle=3（不透明盒子）+ BackColour 半透明黑，字幕阅读性最好
  lines.push(
    "Style: Default," + FONT[lang] + ",21,&H00FFFFFF,&H000000FF,&H00000000,&HA0000000," +
      "0,0,0,0,100,100,0,0,3,2,0,2,24,24,18,1",
  );
  lines.push("");
  lines.push("[Events]");
  lines.push("Format: Layer, Start, End, Style, Name, MarginL, MarginR, MarginV, Effect, Text");

  for (let i = 0; i < cues.length; i++) {
    const c = cues[i];
    const text = c[lang];
    if (!text) continue;
    // 起点减 TRIM（裁掉的起始段），终点为下一条起点，末条到视频末尾
    const start = Math.max(0, c.t - TRIM);
    const next = cues[i + 1];
    const end = next ? Math.max(start + 0.4, next.t - TRIM) : Math.max(start + 0.4, duration - TRIM);
    if (end <= 0) continue;
    lines.push(
      "Dialogue: 0," + assTime(start) + "," + assTime(end) + ",Default,,0,0,0,," + text,
    );
  }
  return lines.join("\n") + "\n";
}

const SKIP = process.argv.includes("--skip-gif");
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
    console.log("[dry] " + src + " -> " + MAP[key]);
    continue;
  }

  // 复制到纯 ASCII 临时路径，避免中文路径在 ffmpeg 下出问题
  const tmpWebm = join(TMP, key + ".webm");
  copyFileSync(src, tmpWebm);

  // 读时间轴（可能不存在 —— 没加 cue 的 spec）
  const cuePath = join(CUE_DIR, key + ".json");
  const cues = existsSync(cuePath) ? JSON.parse(readFileSync(cuePath, "utf8")) : null;

  // 取视频总时长，供最后一条字幕定终点
  let duration = 30;
  const probe = spawnSync(FFMPEG.replace(/ffmpeg(\.exe)?$/i, "ffprobe$1"), [
    "-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", tmpWebm,
  ], { encoding: "utf8" });
  if (probe.status === 0 && probe.stdout.trim()) duration = parseFloat(probe.stdout.trim());

  for (const lang of LANGS) {
    const suffix = lang === "zh" ? "" : "." + lang;
    const out = join(DEST, MAP[key] + suffix + ".gif");
    const tmpGif = join(TMP, key + suffix + ".gif");

    const vfParts = ["fps=" + FPS, "scale=" + WIDTH + ":-2:flags=lanczos"];
    if (cues && cues.length) {
      // 生成 ASS 到临时目录，用相对文件名 + cwd 规避路径转义
      const assName = "sub_" + key + "_" + lang + ".ass";
      writeFileSync(join(TMP, assName), buildAss(cues, lang, duration), "utf8");
      vfParts.push("subtitles=" + assName);
    }

    const args = [
      "-y",
      "-ss", String(TRIM),
      "-i", tmpWebm,
      "-vf", vfParts.join(","),
      "-loop", "0",
      tmpGif,
    ];
    const r = spawnSync(FFMPEG, args, { cwd: TMP, encoding: "utf8" });
    if (r.status !== 0) {
      console.log("失败：" + MAP[key] + suffix);
      console.log((r.stderr || "").slice(-500));
      fail++;
      continue;
    }
    copyFileSync(tmpGif, out);
    ok++;
  }
  console.log("已生成 " + MAP[key] + (cues && cues.length ? "（含字幕 " + cues.length + " 条）" : "（无字幕）"));
}
console.log("\n完成：" + ok + " 个文件，失败：" + fail + " 个");
