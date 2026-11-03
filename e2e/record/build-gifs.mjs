// 批量转码：test-results 下的录屏 → docs/screenshots/manual 下的 GIF。
// 用法：node e2e/record/build-gifs.mjs [--skip-gif]
import { spawnSync } from "node:child_process";
import { existsSync, mkdirSync, readdirSync, copyFileSync } from "node:fs";
import { join } from "node:path";

const FFMPEG = process.env.FFMPEG || "D:/HRSoftstore/LLMchatl/ffmpeg-9.0-full_build/bin/ffmpeg.exe";
const ROOT = process.cwd();
const SRC = join(ROOT, "test-results");
const DEST = join(ROOT, "docs", "screenshots", "manual");
const TMP = join(process.env.TEMP || "/tmp", "dsh-multiver", "rec");

// spec 前缀 → 输出文件名（与手册现有引用一致）
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
  const out = join(DEST, MAP[key] + ".gif");
  if (SKIP) {
    console.log("[dry] " + src + " -> " + out);
    continue;
  }
  // 先复制到纯 ASCII 临时路径，避免中文路径在 ffmpeg 下出问题
  const tmpWebm = join(TMP, key + ".webm");
  copyFileSync(src, tmpWebm);
  const tmpGif = join(TMP, key + ".gif");
  const args = [
    "-y",
    "-ss", "0.8",
    "-i", tmpWebm,
    "-vf", "fps=10,scale=960:-2:flags=lanczos",
    "-loop", "0",
    tmpGif,
  ];
  const r = spawnSync(FFMPEG, args, { encoding: "utf8" });
  if (r.status !== 0) {
    console.log("失败：" + key);
    console.log((r.stderr || "").slice(-400));
    fail++;
    continue;
  }
  copyFileSync(tmpGif, out);
  console.log("已生成 " + out);
  ok++;
}
console.log("\n完成：" + ok + " 张，失败：" + fail + " 张");
