import { ref } from "vue";

// 「运行共享 home 版本时提示」开关——模块级单例：
// 路径设置（PathSettings）与运行前确认（App.vue 的 confirmSharedHome）共享同一份状态，
// 同一次会话内切换开关两处立即联动；同时持久化到 localStorage。
const warnSharedHomeEnabled = ref(localStorage.getItem("dsh-multiver.warnSharedHome") !== "0");

function setWarnSharedHome(val) {
  warnSharedHomeEnabled.value = val;
  localStorage.setItem("dsh-multiver.warnSharedHome", val ? "1" : "0");
}

export function useSharedHomeWarn() {
  return { warnSharedHomeEnabled, setWarnSharedHome };
}
