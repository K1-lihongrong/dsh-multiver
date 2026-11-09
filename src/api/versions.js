import { call, commands } from "./client.js";

export const listRemote = () => call(commands.list_remote());
export const installVersion = (version, registry = null) =>
  call(commands.install_version(version, registry));
export const clearBroken = (version = null) =>
  call(commands.clear_broken(version));
export const uninstallVersion = (version) =>
  call(commands.uninstall_version(version));
export const createShortcut = (version) =>
  call(commands.create_shortcut(version));
export const runVersion = (version) => call(commands.run_version(version));
export const openInBrowser = (version) =>
  call(commands.open_in_browser(version));
export const isVersionOpen = (version) =>
  call(commands.is_version_open(version));
export const listRunningVersions = () => call(commands.list_running_versions());
export const restartVersion = (version) =>
  call(commands.restart_version(version));

// 兼容 re-export：InstalledList 按版本域导入默认版本 / 隔离开关。
export { setDefault } from "./app.js";
export { setIsolated } from "./settings.js";
