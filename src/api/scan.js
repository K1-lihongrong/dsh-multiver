import { call, commands } from "./client.js";

export const scanVersions = (versions) =>
  call(commands.scan_versions(versions));
export const cancelScan = () => call(commands.cancel_scan());
export const scanIsolatedSize = (version) =>
  call(commands.scan_isolated_size(version));
export const runMaintenance = (kind) => call(commands.run_maintenance(kind));
export const setAutoMaintenance = (enabled) =>
  call(commands.set_auto_maintenance(enabled));
