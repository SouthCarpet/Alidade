// Oracle: maker brief 120-p2 and plan 120 p2 acceptance.
#[test]
fn p2_help_escape_retains_opener_and_shortcuts_change_screen() {
    let mut app = test_app();
    let opener = 3usize;
    let _ = app.update(Message::HelpOpened(Some(opener)));
    assert!(app.help_open);
    assert_eq!(app.help_opener, Some(opener));
    let _ = app.update(Message::HelpClosed);
    assert!(!app.help_open);
    assert_eq!(app.help_opener, None);
    let _ = app.update(Message::OpenSettings);
    assert_eq!(app.screen, Screen::Settings);
}
#[test]
fn p2_shortcuts_include_f1_comma_escape_and_undo() {
    use iced::keyboard::{key::Named, Key, Modifiers};
    assert!(matches!(
        shortcut(Key::Named(Named::F1), Modifiers::empty()),
        Some(Message::HelpRequested)
    ));
    assert!(matches!(
        shortcut(Key::Character(",".into()), Modifiers::CTRL),
        Some(Message::OpenSettings)
    ));
    assert!(matches!(
        shortcut(Key::Named(Named::Escape), Modifiers::empty()),
        Some(Message::EscapePressed)
    ));
    assert!(matches!(
        shortcut(Key::Character("z".into()), Modifiers::CTRL),
        Some(Message::UndoShortcutRequested)
    ));
    assert!(shortcut(Key::Character("z".into()), Modifiers::empty()).is_none());
}

#[test]
fn p2_help_request_captures_focus_before_opening() {
    let mut app = test_app();
    assert_eq!(app.update(Message::HelpRequested).units(), 1);
    assert!(!app.help_open);
}
#[test]
fn p2_help_search_and_tab_keep_the_panel_open() {
    let mut app = test_app();
    let _ = app.update(Message::HelpOpened(None));
    let _ = app.update(Message::HelpSearchChanged("jitter".into()));
    let _ = app.update(Message::MoveFocus(false));
    assert_eq!(app.help_query, "jitter");
    assert!(app.help_open);
    let _ = app.update(Message::OpenSettings);
    assert_eq!(app.screen, Screen::Home);
    let _ = app.update(Message::HelpClosed);
    assert!(app.help_query.is_empty());
}

#[test]
fn p2_forward_and_backward_tab_schedule_focus_navigation() {
    let mut app = test_app();
    assert_eq!(app.update(Message::MoveFocus(false)).units(), 1);
    assert_eq!(app.update(Message::MoveFocus(true)).units(), 1);
}

// Fix-round contract: query first, panel second, toast only with Help closed.
#[test]
fn fix1_escape_clears_search_before_closing_help() {
    let mut app = test_app();
    let _ = app.update(Message::HelpOpened(Some(3)));
    app.help_query = "jitter".into();
    let _ = app.update(
        shortcut(
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            iced::keyboard::Modifiers::empty(),
        )
        .unwrap(),
    );
    assert!(app.help_open);
    assert_eq!(app.help_query, "");
    assert_eq!(app.help_opener, Some(3));
    let _ = app.update(
        shortcut(
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            iced::keyboard::Modifiers::empty(),
        )
        .unwrap(),
    );
    assert!(!app.help_open);
    assert_eq!(app.help_opener, None);
}
#[test]
fn fix1_escape_dismisses_undo_without_restoring_target() {
    let mut app = test_app();
    let _ = app.update(Message::TargetRemoved(0));
    let count = app.settings.targets.len();
    let _ = app.update(
        shortcut(
            iced::keyboard::Key::Named(iced::keyboard::key::Named::Escape),
            iced::keyboard::Modifiers::empty(),
        )
        .unwrap(),
    );
    assert!(app.removed_target.is_none());
    assert_eq!(app.settings.targets.len(), count);
}
#[test]
fn fix1_ctrl_z_in_text_input_keeps_target_removed() {
    let mut app = test_app();
    let _ = app.update(Message::TargetRemoved(0));
    let count = app.settings.targets.len();
    let _ = app.update(Message::UndoShortcutChecked(true));
    assert_eq!(app.settings.targets.len(), count);
    assert!(app.removed_target.is_some());
}
#[test]
fn fix1_ctrl_z_outside_text_input_restores_target() {
    let mut app = test_app();
    let count = app.settings.targets.len();
    let _ = app.update(Message::TargetRemoved(0));
    let _ = app.update(Message::UndoShortcutChecked(false));
    assert_eq!(app.settings.targets.len(), count);
    assert!(app.removed_target.is_none());
}
#[test]
fn fix1_about_labels_are_actions_in_both_states() {
    assert_eq!(crate::screens::settings::about_label(false), "Show About");
    assert_eq!(crate::screens::settings::about_label(true), "Hide About");
}

#[test]
fn fix1_undo_shortcut_requests_widget_focus_check() {
    let mut app = test_app();
    assert_eq!(app.update(Message::UndoShortcutRequested).units(), 1);
}
#[test]
fn fix1_escape_in_help_keeps_undo_toast_until_help_closes() {
    let mut app = test_app();
    let _ = app.update(Message::TargetRemoved(0));
    let _ = app.update(Message::HelpOpened(Some(3)));
    let _ = app.update(Message::EscapePressed);
    assert!(!app.help_open);
    assert!(app.removed_target.is_some());
    let _ = app.update(Message::EscapePressed);
    assert!(app.removed_target.is_none());
}
#[test]
fn fix1_close_button_closes_help_with_a_nonempty_search() {
    let mut app = test_app();
    let _ = app.update(Message::HelpOpened(Some(3)));
    app.help_query = "jitter".into();
    let _ = app.update(Message::HelpClosed);
    assert!(!app.help_open);
    assert_eq!(app.help_opener, None);
}
