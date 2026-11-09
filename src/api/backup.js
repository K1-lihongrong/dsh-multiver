import { call, commands } from "./client.js";

export const exportBackup = (file) => call(commands.export_backup(file));
export const previewImport = (file) => call(commands.preview_import(file));
export const applyImport = (file) => call(commands.apply_import(file));
