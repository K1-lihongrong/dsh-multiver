import { invoke } from "@tauri-apps/api/core";

export function sharedHomeHealth(version) {
  return invoke("ds_shared_home_health", { version });
}

export function classifyPluginFailure(output) {
  return invoke("ds_classify_plugin_failure", { output });
}
