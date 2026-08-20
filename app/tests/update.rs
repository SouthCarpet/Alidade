#[test]
fn tag_comparison_handles_the_v_prefix_and_ordering() {
    assert!(alidade_app::update::is_newer("0.1.0", "v0.2.0"));
    assert!(alidade_app::update::is_newer("0.1.0", "0.1.1"));
    assert!(!alidade_app::update::is_newer("0.2.0", "v0.1.9"));
    assert!(!alidade_app::update::is_newer("0.1.0", "v0.1.0"));
}

#[test]
fn a_malformed_tag_is_never_newer() {
    assert!(!alidade_app::update::is_newer("0.1.0", "latest"));
    assert!(!alidade_app::update::is_newer("0.1.0", ""));
}

#[test]
fn more_malformed_tags_are_never_newer() {
    assert!(!alidade_app::update::is_newer("0.1.0", "v"));
    assert!(!alidade_app::update::is_newer("0.1.0", "0.1"));
    assert!(!alidade_app::update::is_newer("0.1.0", "0.1.0.1"));
    assert!(!alidade_app::update::is_newer("0.1.0", "0.one.0"));
    assert!(!alidade_app::update::is_newer("0.1.0", "-1.0.0"));
}

#[test]
fn a_malformed_current_version_is_never_behind() {
    // If we cannot even parse our own version, the safe answer is "no
    // update" — never a false positive built on a guess.
    assert!(!alidade_app::update::is_newer("not-a-version", "v9.9.9"));
}

#[test]
fn major_and_minor_bumps_count_as_newer() {
    assert!(alidade_app::update::is_newer("0.9.9", "1.0.0"));
    assert!(alidade_app::update::is_newer("1.2.3", "1.3.0"));
}

#[test]
fn an_uppercase_v_prefix_is_also_tolerated() {
    assert!(alidade_app::update::is_newer("0.1.0", "V0.2.0"));
}

#[test]
fn a_prerelease_tag_is_never_offered_as_an_update() {
    // Decision documented on `is_newer`: the quiet update check only ever
    // offers stable releases. A release candidate can carry a higher
    // numeric core than the running version and still must not surface
    // through the same "update available" prompt used for stable builds.
    assert!(!alidade_app::update::is_newer("0.1.0", "0.2.0-rc1"));
    assert!(!alidade_app::update::is_newer("0.1.0", "v0.2.0-beta.1"));
}

#[test]
fn equal_versions_are_never_newer() {
    assert!(!alidade_app::update::is_newer("0.1.0", "0.1.0"));
}

#[test]
fn release_struct_matches_the_documented_shape() {
    let release = alidade_app::update::Release {
        tag: "v1.2.3".to_string(),
        url: "https://github.com/SouthCarpet/Alidade/releases/tag/v1.2.3".to_string(),
        notes: "Fixes".to_string(),
    };
    assert_eq!(release.tag, "v1.2.3");
    assert_eq!(release.url, "https://github.com/SouthCarpet/Alidade/releases/tag/v1.2.3");
    assert_eq!(release.notes, "Fixes");
}

#[test]
fn status_error_message_is_a_quiet_line_not_a_raw_debug_dump() {
    let err = alidade_app::update::UpdateError::Status(403);
    let message = err.to_string();
    assert!(message.contains("403"));
    assert!(!message.to_lowercase().contains("panic"));
}
