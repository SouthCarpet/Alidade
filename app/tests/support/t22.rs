// Oracle: maker brief 120-p2 and plan 120 p2 acceptance.
#[test]
fn p2_remove_undo_restores_index_fields_and_raw_port() {
    let mut app = test_app();
    app.settings.targets = vec![
        TargetSpec {
            name: "first".into(),
            probe: Probe::Icmp {
                host: "1.1.1.1".into(),
            },
            enabled: true,
            verified: true,
        },
        TargetSpec {
            name: "middle".into(),
            probe: Probe::TcpConnect {
                host: "example.net".into(),
                port: 443,
            },
            enabled: false,
            verified: false,
        },
        TargetSpec {
            name: "last".into(),
            probe: Probe::Icmp {
                host: "8.8.8.8".into(),
            },
            enabled: true,
            verified: true,
        },
    ];
    app.target_port_text = vec![String::new(), "00443x".into(), String::new()];
    let _ = app.update(Message::TargetRemoved(1));
    assert_eq!(app.settings.targets.len(), 2);
    assert!(app.removed_target.is_some());
    let _ = app.update(Message::UndoTargetRemoval);
    assert_eq!(app.settings.targets[0].name, "first");
    assert_eq!(app.settings.targets[1].name, "middle");
    assert_eq!(app.settings.targets[2].name, "last");
    assert!(!app.settings.targets[1].enabled);
    assert!(!app.settings.targets[1].verified);
    assert_eq!(
        app.settings.targets[1].probe,
        Probe::TcpConnect {
            host: "example.net".into(),
            port: 443
        }
    );
    assert_eq!(app.target_port_text, vec!["", "00443x", ""]);
    assert!(app.removed_target.is_none());
}
#[test]
fn p2_expiry_drops_undo_and_stale_expiry_keeps_new_removal() {
    let mut app = test_app();
    app.target_port_text = app.settings.targets.iter().map(port_text_for).collect();
    let _ = app.update(Message::TargetRemoved(0));
    let old = app.removal_generation;
    let _ = app.update(Message::TargetRemoved(0));
    let _ = app.update(Message::UndoRemovalExpired(old));
    assert!(app.removed_target.is_some());
    let _ = app.update(Message::UndoRemovalExpired(app.removal_generation));
    assert!(app.removed_target.is_none());
    let count = app.settings.targets.len();
    let _ = app.update(Message::UndoTargetRemoval);
    assert_eq!(app.settings.targets.len(), count);
}
#[test]
fn p2_invalid_remove_does_not_replace_undo() {
    let mut app = test_app();
    let _ = app.update(Message::TargetRemoved(0));
    let generation = app.removal_generation;
    let _ = app.update(Message::TargetRemoved(usize::MAX));
    assert_eq!(app.removal_generation, generation);
}
#[test]
fn p2_ctrl_z_reaches_the_undo_action() {
    use iced::keyboard::{Key, Modifiers};
    assert!(matches!(
        shortcut(Key::Character("z".into()), Modifiers::CTRL),
        Some(Message::UndoTargetRemoval)
    ));
}
