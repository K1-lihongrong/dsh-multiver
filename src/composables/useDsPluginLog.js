import { ref } from "vue";

// 插件面板的实时进度行。
//
// 安装对话框（installing 阶段）与面板底部的日志区显示同一份内容，
// 由 DsPluginPanel（壳）创建后 provide 注入，子组件用 inject 读取。
export const DS_PLUGIN_LOG_KEY = Symbol("dsPluginLog");

export function useDsPluginLog() {
  const logLines = ref([]);
  const showLog = ref(false);

  function resetLog() {
    logLines.value = [];
  }

  /// 追加一行进度。只保留最近 500 行，避免长时间安装把内存撑大。
  function pushLogLine(line) {
    logLines.value.push(line);
    if (logLines.value.length > 500) {
      logLines.value.splice(0, logLines.value.length - 500);
    }
  }

  return { logLines, showLog, resetLog, pushLogLine };
}
