import { call, commands } from "./client.js";

export const getState = () => call(commands.get_state());
export const listInstalled = () => call(commands.list_installed());
export const setDefault = (version, force = null) =>
  call(commands.set_default(version, force));
export const openUrl = (url) => call(commands.open_url(url));
export const openDir = (which) => call(commands.open_dir(which));
export const checkUpdate = () => call(commands.check_update());
export const performUpdate = (url, sha256 = null) =>
  call(commands.perform_update(url, sha256));
export const exitApp = () => call(commands.exit_app());
export const takeOpenSettings = () => call(commands.take_open_settings());
export const getLaunchMode = () => call(commands.get_launch_mode());
export const getLaunchError = () => call(commands.get_launch_error());
export const showMainWindow = () => call(commands.show_main_window());
export const logFrontend = (msg) => call(commands.log_frontend(msg));

// 兼容 re-export（运行时提示属于 env 模块，但 App 依赖 app 聚合入口）。
export { dismissRuntimePrompt } from "./env.js";
