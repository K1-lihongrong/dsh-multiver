import { call, commands } from "./client.js";

export const setRoot = (root) => call(commands.set_root(root));
export const setUseOfficialHome = (enabled) =>
  call(commands.set_use_official_home(enabled));
export const getOfficialHome = () => call(commands.get_official_home());
export const setIsolated = (version, isolated) =>
  call(commands.set_isolated(version, isolated));
