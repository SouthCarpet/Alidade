// Oracle: maker brief 120-p2 and plan 120 p2 acceptance.
#[test]
fn p2_boot_opted_in_starts_a_check() {
    let mut app = test_app();
    app.ui_settings.check_updates = true;
    let task = app.request_update_check();
    assert!(app.update_checking);
    assert_eq!(task.units(), 1);
}
#[test]
fn p2_about_open_opted_in_starts_a_check() {
    let mut app = test_app();
    app.ui_settings.check_updates = true;
    let _ = app.update(Message::AboutToggled);
    assert!(app.about_open);
    assert!(app.update_checking);
}
#[test]
fn p2_check_now_opted_in_starts_a_check() {
    let mut app = test_app();
    app.ui_settings.check_updates = true;
    let _ = app.update(Message::CheckUpdatesNow);
    assert!(app.update_checking);
}
#[test]
fn p2_opted_out_blocks_boot_about_and_button() {
    let mut app = test_app();
    assert_eq!(app.request_update_check().units(), 0);
    let _ = app.update(Message::AboutToggled);
    let _ = app.update(Message::CheckUpdatesNow);
    assert!(!app.update_checking);
    assert_eq!(
        app.update_status(),
        "Update checks are off. Enable them below to check GitHub."
    );
}
#[test]
fn p2_check_result_is_visible_and_cannot_reappear_after_opt_out() {
    let mut app = test_app();
    app.ui_settings.check_updates = true;
    let _ = app.update(Message::UpdateChecked(None));
    assert_eq!(
        app.update_status(),
        "Last result: you have the latest version."
    );
    let _ = app.update(Message::UpdateCheckFailed("HTTP 403".into()));
    assert_eq!(app.update_status(), "Last result: check failed (HTTP 403).");
    let _ = app.update(Message::SettingChanged(SettingField::CheckUpdates(false)));
    let _ = app.update(Message::UpdateChecked(Some(update::Release {
        tag: "v9.0.0".into(),
        url: "https://github.com/SouthCarpet/Alidade/releases".into(),
        notes: String::new(),
    })));
    assert!(app.available_update.is_none());
}
#[test]
fn p2_quick_theme_uses_effective_system_mode() {
    let mut app = test_app();
    let _ = app.update(Message::SystemThemeChanged(iced::theme::Mode::Dark));
    let _ = app.update(Message::QuickThemeSwitch);
    assert_eq!(app.ui_settings.theme, ThemeMode::Light);
    let _ = app.update(Message::QuickThemeSwitch);
    assert_eq!(app.ui_settings.theme, ThemeMode::Dark);
}
#[test]
fn p2_about_close_does_not_start_another_check() {
    let mut app = test_app();
    app.about_open = true;
    app.ui_settings.check_updates = true;
    assert_eq!(app.update(Message::AboutToggled).units(), 0);
    assert!(!app.about_open);
}
#[test]
fn p2_running_check_does_not_send_duplicate_requests() {
    let mut app = test_app();
    app.ui_settings.check_updates = true;
    let _ = app.update(Message::CheckUpdatesNow);
    assert_eq!(app.update(Message::CheckUpdatesNow).units(), 0);
}
