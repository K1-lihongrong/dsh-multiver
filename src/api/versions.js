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
export const runVersion = (version, instance = null) =>
  call(commands.run_version(version, instance));
export const openInBrowser = (version, instance = null) =>
  call(commands.open_in_browser(version, instance));
export const isVersionOpen = (version) =>
  call(commands.is_version_open(version));
export const isInstanceOpen = (instance) =>
  call(commands.is_instance_open(instance));
export const listRunningVersions = () => call(commands.list_running_versions());
export const listRunningTargets = () => call(commands.list_running_targets());
export const restartVersion = (version, instance = null) =>
  call(commands.restart_version(version, instance));

// 兼容 re-export：InstalledList 按版本域导入默认版本 / 隔离开关。
export { setDefault } from "./app.js";
export { setIsolated } from "./settings.js";
