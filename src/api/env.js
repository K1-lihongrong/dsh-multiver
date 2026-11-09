import { call, commands } from "./client.js";

export const checkEnv = (full = true) => call(commands.check_env(full));
export const runtimeStatus = () => call(commands.runtime_status());
export const setupRuntime = () => call(commands.setup_runtime());
export const dismissRuntimePrompt = () => call(commands.dismiss_runtime_prompt());
