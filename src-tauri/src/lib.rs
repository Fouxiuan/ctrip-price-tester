mod opencli;
mod storage;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            storage::setup(app.handle()).map_err(|error| -> Box<dyn std::error::Error> { error.into() })?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            opencli::check_environment,
            opencli::search_ctrip_hotels,
            opencli::get_ctrip_price_calendar,
            opencli::test_ctrip_price,
            storage::load_searched_hotels,
            storage::update_hotel_min_price,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run ctrip price tester");
}
