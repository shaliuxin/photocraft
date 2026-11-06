//! Use the actual platform UI-language API and the production (non-cfg(test)) Auto path.
#![cfg(not(target_arch = "wasm32"))]

use egui_kittest::{Harness, kittest::Queryable};
use photocraft_ui_egui::{PhotocraftApp, i18n, menu_keys};

#[test]
fn native_first_launch_follows_os_ui_preferences() {
    let tags: Vec<String> = match std::env::var("PHOTOCRAFT_LOCALE").ok().filter(|tag| !tag.trim().is_empty()) {
        Some(tag) => vec![tag],
        None => sys_locale::get_locales().take(64).collect(),
    };
    let expected = tags
        .iter()
        .filter(|tag| tag.len() <= 128 && tag.trim().bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.' | b'@')))
        .find_map(|tag| i18n::lang_from_tag(tag.trim()))
        .unwrap_or(i18n::Lang::EN);
    eprintln!("native UI-language preferences: {tags:?}; resolved: {}", expected.code());
    let mut h = Harness::builder().with_size(egui::vec2(1200.0, 800.0)).with_max_steps(64).build_eframe(|cc| {
        PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
        PhotocraftApp::new(photocraft_engine::Session::new(), Default::default())
    });
    h.run_steps(4);
    assert_eq!(h.state().session.prefs().interface.language, "auto");
    assert_eq!(i18n::current(), expected, "native language preferences: {tags:?}");
    // The File heading is shown through `menu_keys::localized`, which appends a Windows-style
    // mnemonic suffix to labels with no typeable letter (e.g. "文件(F)", #5863). So query the
    // exact label the bar draws for "File", not the bare translation (kittest's `query_by_label`
    // is an exact match, so "文件" alone would never find "文件(F)").
    let file_label = i18n::tr(expected, "File").to_string();
    let (shown, _) = menu_keys::localized(&[file_label], &["File"], &[Some('F')]);
    assert!(h.query_by_label(&shown[0]).is_some(), "the first-launch menu must use the OS UI language (File heading {shown:?})");
}
