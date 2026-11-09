import { invoke } from "@tauri-apps/api/core";

export function listJobs(limit = 20) {
  return invoke("list_jobs", { limit });
}
