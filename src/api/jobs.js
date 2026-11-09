import { call, commands } from "./client.js";

export const listJobs = (limit = 20) => call(commands.list_jobs(limit));
