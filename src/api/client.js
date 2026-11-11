// A2-8: 生成 bindings 的统一调用层。
// tauri-specta 对 Result 命令生成 { status: "ok" | "error" }，这里解包成
// 原来的「成功返回值 / 抛出后端错误字符串」语义，避免组件行为变化。
import { commands } from "./bindings.ts";

export { commands };

export function unwrap(result) {
  if (result && typeof result === "object" && "status" in result) {
    if (result.status === "error") throw result.error;
    return result.data;
  }
  return result;
}

export async function call(promise) {
  return unwrap(await promise);
}

export function notifyLogged(promise) {
  return promise?.catch?.(() => {}) ?? undefined;
}
