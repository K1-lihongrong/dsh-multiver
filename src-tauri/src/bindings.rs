//! A2-8: tauri-specta command registry + TypeScript binding export.
//!
//! This is the single source of truth for the Tauri invoke handler: the
//! runtime handler and the committed TypeScript bindings are generated from
//! the same command list. Run `UPDATE_BINDINGS=1 cargo test --lib
//! generated_bindings_are_up_to_date` after changing command signatures.
#![allow(dead_code)]

use specta_typescript::Typescript;
use tauri_specta::{collect_commands, Builder, Casing};

pub(crate) fn builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new()
        .function_casing(Casing::SnakeCase)
        .commands(collect_commands![
            crate::commands::get_state,
            crate::commands::list_installed,
            crate::commands::list_remote,
            crate::commands::install_version,
            crate::commands::clear_broken,
            crate::commands::uninstall_version,
            crate::commands::set_default,
            crate::commands::run_version,
            crate::commands::open_in_browser,
            crate::commands::create_shortcut,
            crate::commands::restart_version,
            crate::commands::set_root,
            crate::commands::open_dir,
            crate::commands::open_url,
            crate::commands::check_env,
            crate::commands::runtime_status,
            crate::commands::setup_runtime,
            crate::commands::dismiss_runtime_prompt,
            crate::commands::set_isolated,
            crate::commands::set_use_official_home,
            crate::commands::get_official_home,
            crate::commands::scan_versions,
            crate::commands::cancel_scan,
            crate::commands::run_maintenance,
            crate::commands::set_auto_maintenance,
            crate::commands::scan_isolated_size,
            crate::commands::copy_shared_to_isolated,
            crate::commands::clear_isolated_data,
            crate::commands::open_isolated_dir,
            crate::commands::log_frontend,
            crate::commands::check_update,
            crate::commands::perform_update,
            crate::commands::exit_app,
            crate::commands::export_backup,
            crate::commands::preview_import,
            crate::commands::apply_import,
            crate::commands::get_launch_mode,
            crate::commands::get_launch_error,
            crate::commands::take_open_settings,
            crate::commands::show_main_window,
            crate::commands::is_version_open,
            crate::commands::is_instance_open,
            crate::commands::list_running_versions,
            crate::commands::list_running_targets,
            crate::commands::list_instances,
            crate::commands::instance_create,
            crate::commands::instance_rename,
            crate::commands::instance_remove,
            crate::commands::attach_instance,
            crate::commands::detach_instance,
            crate::commands::set_selected_target,
            crate::commands::scan_instance_size,
            crate::commands::open_instance_dir,
            crate::commands::ds_list_plugins,
            crate::commands::ds_plugin_action,
            crate::commands::ds_plugin_update_batch,
            crate::commands::ds_init_profile,
            crate::commands::ds_open_profile_dir,
            crate::commands::ds_profile_info,
            crate::commands::ds_exemption_action,
            crate::commands::ds_approve_builds,
            crate::commands::ds_open_plugin_logs,
            crate::commands::ds_check_updates,
            crate::commands::ds_shared_home_health,
            crate::commands::ds_classify_plugin_failure,
            crate::commands::list_jobs,
            crate::commands::ds_inspect_spec,
            crate::commands::ds_probe_registries,
            crate::commands::ds_plugin_catalog,
            crate::commands::ds_plugin_catalog_check,
        ])
}

pub(crate) const BINDINGS_PATH: &str = "../src/api/bindings.ts";

/// tauri-specta rc.26 emits `package` as a TypeScript parameter name for
/// `ds_plugin_action`; `package` is a reserved word in TS modules. Keep the IPC
/// argument key (`package`, required by the existing command/E2E contract) and
/// sanitize only the generated local identifier.
fn sanitize_generated_bindings(mut text: String) -> String {
    text = text.replace("package: string", "package_: string");
    text = text.replace(
        "{ version, profile, action, package, registry }",
        "{ version, profile, action, package: package_, registry }",
    );
    // `ds_exemption_action` declares `#[tauri::command(rename_all = "snake_case")]`
    // but the TS exporter still camelCases parameter names. Preserve the backend
    // IPC contract (snake_case, asserted by E2E) in the generated invoke args.
    text = text.replace(
        "packageVersion: string, dshVersion: string",
        "package_version: string, dsh_version: string",
    );
    text = text.replace(
        "{ version, profile, action, packageVersion, dshVersion }",
        "{ version, profile, action, package_version, dsh_version }",
    );
    text
}

pub(crate) fn export(path: &str) -> Result<(), String> {
    let tmp = format!("{path}.generated.tmp");
    builder()
        .dangerously_cast_bigints_to_number()
        .export(Typescript::default(), &tmp)
        .map_err(|e| e.to_string())?;
    let generated = std::fs::read_to_string(&tmp).map_err(|e| e.to_string())?;
    std::fs::write(path, sanitize_generated_bindings(generated)).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&tmp);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn sanitizes_reserved_package_argument_without_changing_ipc_key() {
        let input = "ds_plugin_action: (version: string, profile: string, action: string, package: string, registry: string | null) => invoke(\"ds_plugin_action\", { version, profile, action, package, registry })".to_string();
        let out = super::sanitize_generated_bindings(input);
        assert!(out.contains("package_: string"));
        assert!(out.contains("package: package_"));
    }
}
