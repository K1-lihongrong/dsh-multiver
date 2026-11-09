import { call, commands } from "./client.js";

export const copySharedToIsolated = (version) =>
  call(commands.copy_shared_to_isolated(version));
export const clearIsolatedData = (version) =>
  call(commands.clear_isolated_data(version));
export const openIsolatedDir = (version) =>
  call(commands.open_isolated_dir(version));
