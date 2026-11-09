import { call, commands } from "./client.js";

export const dsListPlugins = (version) =>
  call(commands.ds_list_plugins(version));
export const dsProfileInfo = (version, profile) =>
  call(commands.ds_profile_info(version, profile));
export const dsExemptionAction = (version, profile, action, packageVersion, dshVersion) =>
  call(commands.ds_exemption_action(version, profile, action, packageVersion, dshVersion));
export const dsApproveBuilds = (version, profile, packages) =>
  call(commands.ds_approve_builds(version, profile, packages));
export const dsOpenPluginLogs = (version, profile) =>
  call(commands.ds_open_plugin_logs(version, profile));
export const dsInspectSpec = (version, profile, spec, registry = null) =>
  call(commands.ds_inspect_spec(version, profile, spec, registry));
export const dsProbeRegistries = () => call(commands.ds_probe_registries());
export const dsPluginAction = (version, profile, action, packageName, registry = null) =>
  call(commands.ds_plugin_action(version, profile, action, packageName, registry));
export const dsInitProfile = (version, profile) =>
  call(commands.ds_init_profile(version, profile));
export const dsOpenProfileDir = (version, profile) =>
  call(commands.ds_open_profile_dir(version, profile));
export const dsClassifyPluginFailure = (output) =>
  call(commands.ds_classify_plugin_failure(output));
export const dsSharedHomeHealth = (version) =>
  call(commands.ds_shared_home_health(version));
export const dsCheckUpdates = (version, profile) =>
  call(commands.ds_check_updates(version, profile));
export const dsPluginUpdateBatch = (version, profile, targets) =>
  call(commands.ds_plugin_update_batch(version, profile, targets));
export const dsPluginCatalog = () => call(commands.ds_plugin_catalog());
export const dsPluginCatalogCheck = (force = false) =>
  call(commands.ds_plugin_catalog_check(!!force));

// 兼容旧导出的命名；新代码优先使用 ds* 前缀（与 Tauri 命令同名，便于检索）。
export const classifyPluginFailure = dsClassifyPluginFailure;
export const sharedHomeHealth = dsSharedHomeHealth;
