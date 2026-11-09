#!/usr/bin/env node
// A2-8: command contract check.
// 1) src-tauri/src/bindings.rs collect_commands![...] is the single source of
//    truth used by both the Tauri invoke handler and the TS binding generator.
// 2) src/api/bindings.ts must contain exactly those commands.
// 3) src/api/*.js must wrap every backend command via commands.xxx(...).
// 4) No naked invoke("name") calls may remain outside the generated bindings.
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

// 1) Backend command list: bindings.rs is the single source of truth.
const bindingsRs = readFileSync(join(ROOT, "src-tauri/src/bindings.rs"), "utf8");
const backend = new Set(
  [...bindingsRs.matchAll(/commands::([a-z0-9_]+)/g)].map((m) => m[1]),
);
if (!backend.size) {
  console.error("No collect_commands![...] command list found in bindings.rs");
  process.exit(1);
}

// 2) Generated bindings: parse the IPC command name from each generated call.
const bindingsPath = join(ROOT, "src/api/bindings.ts");
let bindingsTs = "";
try {
  bindingsTs = readFileSync(bindingsPath, "utf8");
} catch {
  console.error("Missing src/api/bindings.ts; run npm run bindings:generate");
  process.exit(1);
}
const generated = new Set();
for (const m of bindingsTs.matchAll(/\binvoke(?:<[^>]*>)?\(\s*["'`]([a-z0-9_]+)["'`]/g)) {
  generated.add(m[1]);
}
for (const m of bindingsTs.matchAll(/__TAURI_INVOKE(?:<[^>]*>)?\(\s*["'`]([a-z0-9_]+)["'`]/g)) {
  generated.add(m[1]);
}
for (const m of bindingsTs.matchAll(/__TAURI_INTERNALS__\.invoke(?:<[^>]*>)?\(\s*["'`]([a-z0-9_]+)["'`]/g)) {
  generated.add(m[1]);
}
if (!generated.size) {
  console.error("No generated command calls found in bindings.ts");
  process.exit(1);
}

// 3) src/api wrappers.
const apiFiles = walk(join(ROOT, "src/api"), [".js"]);
const wrapped = new Set();
for (const f of apiFiles) {
  const txt = readFileSync(f, "utf8");
  for (const m of txt.matchAll(/\bcommands\.([a-z0-9_]+)\s*\(/g)) wrapped.add(m[1]);
}

// 4) Naked invoke calls in app code.
const frontFiles = walk(join(ROOT, "src"), [".js", ".vue", ".ts"]);
const naked = [];
for (const f of frontFiles) {
  const rel = f.replace(ROOT, "").replace(/\\/g, "/");
  if (rel.endsWith("/api/bindings.ts") || rel.endsWith("/api/client.js")) continue;
  const txt = readFileSync(f, "utf8");
  for (const m of txt.matchAll(/\binvoke\s*\(/g)) {
    const line = txt.slice(0, m.index).split("\n").length;
    naked.push(`${rel}:${line}`);
  }
}

const onlyBackend = [...backend].filter((c) => !generated.has(c)).sort();
const onlyGenerated = [...generated].filter((c) => !backend.has(c)).sort();
const missingWrappers = [...backend].filter((c) => !wrapped.has(c)).sort();
const unknownWrappers = [...wrapped].filter((c) => !backend.has(c)).sort();

console.log(`Backend commands: ${backend.size}`);
console.log(`Generated bindings: ${generated.size}`);
console.log(`API wrappers: ${wrapped.size}`);
if (onlyBackend.length) console.error("Missing from bindings.ts: " + onlyBackend.join(", "));
if (onlyGenerated.length) console.error("Unknown commands in bindings.ts: " + onlyGenerated.join(", "));
if (missingWrappers.length) console.error("Missing src/api wrappers: " + missingWrappers.join(", "));
if (unknownWrappers.length) console.error("Unknown src/api commands: " + unknownWrappers.join(", "));
if (naked.length) console.error("Naked invoke calls remain:\n  " + naked.join("\n  "));

if (onlyBackend.length || onlyGenerated.length || missingWrappers.length || unknownWrappers.length || naked.length) {
  process.exit(1);
}
console.log("Command contract OK (bindings.rs -> bindings.ts -> src/api -> no naked invoke).");
