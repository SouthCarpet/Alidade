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
        Some(Message::HelpClosed)
    ));
    assert!(matches!(
        shortcut(Key::Character("z".into()), Modifiers::CTRL),
        Some(Message::UndoTargetRemoval)
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
