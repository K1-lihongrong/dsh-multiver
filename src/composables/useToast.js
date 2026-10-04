import { ref } from "vue";

// 全局通知（toast）——模块级单例：
// 所有 useToast() 调用共享同一份 toast 状态，任意处 notify() 都驱动同一个 UI。
// 抽为 composable 是为了让拆分出的组件也能直接 notify，无需层层传事件。

const toast = ref("");
const toastExpanded = ref(false);
let toastTimer = null;

/// 显示一条通知。短消息 6 秒后自动消失；长消息（含换行/超 60 字）不自动消失，由用户关闭。
function notify(msg) {
  toast.value = msg;
  toastExpanded.value = false;
  if (toastTimer) clearTimeout(toastTimer);
  const isLong = String(msg).length > 60 || String(msg).includes("\n");
  if (!isLong) {
    toastTimer = setTimeout(() => { toast.value = ""; }, 6000);
  }
}

function dismissToast() {
  if (toastTimer) clearTimeout(toastTimer);
  toast.value = "";
  toastExpanded.value = false;
}

function toggleToast() {
  toastExpanded.value = !toastExpanded.value;
  if (toastExpanded.value && toastTimer) clearTimeout(toastTimer);
}

export function useToast() {
  return { toast, toastExpanded, notify, dismissToast, toggleToast };
}
