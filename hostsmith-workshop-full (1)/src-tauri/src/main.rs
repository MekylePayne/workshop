// HostSmith Workshop - Tauri 2 shell.
//
// Review 6.2 rule: this shell stays a thin adapter. Every command lives in
// commands.rs and delegates to a headless, tested crate; no business logic
// lives here.

mod commands;
mod state;

use state::AppState;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::pick_folder,
            commands::pick_file,
            commands::open_project,
            commands::close_project,
            commands::rescan_project,
            commands::import_zip,
            commands::list_project_dir,
            commands::read_text_file,
            commands::read_binary_file_base64,
            commands::start_preview,
            commands::stop_preview,
            commands::propose_text_edit,
            commands::propose_attribute_edit,
            commands::propose_media_swap,
            commands::apply_edited_files,
            commands::import_media,
            commands::undo_step,
            commands::journal_entries,
            commands::list_backups,
            commands::restore_backup,
            commands::analyze_seo,
            commands::plan_seo_fix,
            commands::favicon_detect,
            commands::favicon_generate,
            commands::validate_structure,
            commands::tool_format_json,
            commands::tool_minify_json,
            commands::tool_validate_json,
            commands::tool_minify_html,
            commands::tool_minify_css,
            commands::tool_minify_js,
            commands::tool_qr_svg,
            commands::tool_palette,
            commands::tool_image_info,
            commands::tool_image_resize,
            commands::tool_image_crop,
            commands::tool_image_rotate,
            commands::tool_image_convert,
            commands::tool_png_optimize,
            commands::tool_jpeg_optimize,
            commands::get_settings,
            commands::set_badge_signature,
            commands::set_badge_snippet,
            commands::apply_badge,
            commands::cf_save_token,
            commands::cf_has_token,
            commands::cf_disconnect,
            commands::cf_accounts,
            commands::cf_pages_projects,
            commands::cf_create_pages_project,
            commands::cf_deploy_pages,
            commands::cf_workers_scripts,
            commands::cf_upload_worker,
            commands::cf_delete_worker,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HostSmith Workshop");
}
