import { call, commands } from "./client.js";

export const listInstances = () => call(commands.list_instances());
export const createInstance = (name, attach = []) =>
  call(commands.instance_create(name, attach));
export const renameInstance = (id, name) =>
  call(commands.instance_rename(id, name));
export const removeInstance = (id) => call(commands.instance_remove(id));
export const attachInstance = (version, id) =>
  call(commands.attach_instance(version, id));
export const detachInstance = (version, id) =>
  call(commands.detach_instance(version, id));
export const setSelectedTarget = (version, instance) =>
  call(commands.set_selected_target(version, instance));
export const scanInstanceSize = (id) => call(commands.scan_instance_size(id));
export const openInstanceDir = (id) => call(commands.open_instance_dir(id));
