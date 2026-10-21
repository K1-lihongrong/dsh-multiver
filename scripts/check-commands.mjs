#!/usr/bin/env node
// D 技术债：命令契约核对。
// 比对两处：lib.rs 的 generate_handler! 注册的命令 vs 前端 invoke("xxx") 用到的命令。
// 任一侧多出/缺少都报错（退出码 1）。
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");

function walk(dir, exts, out = []) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) walk(p, exts, out);
    else if (exts.some((e) => name.endsWith(e))) out.push(p);
  }
  return out;
}

// 1) 后端：lib.rs 里 generate_handler![ commands::a, commands::b, ... ]
const libRs = readFileSync(join(ROOT, "src-tauri/src/lib.rs"), "utf8");
const handlerBlock = libRs.match(/generate_handler!\[([\s\S]*?)\]/);
if (!handlerBlock) {
  console.error("未在 lib.rs 找到 generate_handler![...]");
  process.exit(1);
}
const backend = new Set(
  [...handlerBlock[1].matchAll(/commands::([a-z0-9_]+)/g)].map((m) => m[1])
);

// 2) 前端：所有 invoke("name" ...)
const frontFiles = walk(join(ROOT, "src"), [".js", ".vue"]);
const frontend = new Set();
for (const f of frontFiles) {
  const txt = readFileSync(f, "utf8");
  for (const m of txt.matchAll(/invoke\(\s*["'\`]([a-z0-9_]+)["'\`]/g)) frontend.add(m[1]);
}

const onlyBackend = [...backend].filter((c) => !frontend.has(c)).sort();
const onlyFrontend = [...frontend].filter((c) => !backend.has(c)).sort();

console.log("后端注册: " + backend.size + " 个");
console.log("前端调用: " + frontend.size + " 个");
if (onlyBackend.length) console.log("仅后端有（前端未调用，可能是 CLI/内部用）:\n  " + onlyBackend.join(", "));
if (onlyFrontend.length) console.log("仅前端有（后端未注册 → 运行会失败！）:\n  " + onlyFrontend.join(", "));

if (onlyFrontend.length) {
  console.error("\n命令契约不一致：前端调用了未注册的命令");
  process.exit(1);
}
console.log("\n命令契约一致（前端所有 invoke 均已注册）");
