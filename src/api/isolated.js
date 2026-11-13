import { call, commands } from "./client.js";

export const copySharedToIsolated = (version, instance = null) =>
  call(commands.copy_shared_to_isolated(version, instance));
export const clearIsolatedData = (version, instance = null) =>
  call(commands.clear_isolated_data(version, instance));
export const openIsolatedDir = (version, instance = null) =>
  call(commands.open_isolated_dir(version, instance));
