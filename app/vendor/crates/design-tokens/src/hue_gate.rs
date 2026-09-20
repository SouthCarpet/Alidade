// Hue-collision gate (item D, narrowed by thesis check F4): pure functions
// shared between build.rs (the real gate, panics the build) and
// tests/gate.rs (exercises the same logic on in-memory data — no file I/O,
// no subprocess, so a test genuinely proves what the build would do).
// Self-contained (no `crate::` imports), included verbatim into both
// build.rs and `lib.rs`'s public `hue_gate` module, exactly like
// `color.rs` and `schema.rs` are shared — one implementation, no drift.

/// One app's accent hue plus whichever `accepted_hue_overlaps` entries its
/// token file carries, as `(with, reason)` pairs. `with` names a status
/// ("danger"/"warning"/"success") or another app's kebab name.
pub struct AppHue {
    pub name: String,
    pub accent_hue: f64,
    pub accepted: Vec<(String, String)>,
}

/// Circular hue distance in degrees, 0-180 (same formula as build.rs's
/// dataviz slot rule).
pub fn circular_diff(a: f64, b: f64) -> f64 {
    let d = (a - b).abs() % 360.0;
    if d > 180.0 {
        360.0 - d
    } else {
        d
    }
}

fn accepted_for(app: &AppHue, with: &str) -> bool {
    app.accepted.iter().any(|(w, _)| w == with)
}

/// Checks every accepted_hue_overlaps reason is non-empty, every app's
/// accent hue against the three status hues (collision at or under
/// `accent_exclusion_deg`), and every app pair's accent hues (collision
/// strictly under `app_to_app_deg` — D10/F4 narrows this from the full
/// `accent_exclusion_deg`; `gallery-proof` is excluded from this half of
/// the check, it is a proof sandbox, not a shipped identity). An
/// `accepted_hue_overlaps` entry on either side of a pair silences that
/// pair. Returns the first violation as `Err(message)`; `Ok(())` means
/// every app is clear or accepted.
pub fn check_hue_collisions(
    apps: &[AppHue],
    statuses: &[(&str, f64)],
    accent_exclusion_deg: f64,
    app_to_app_deg: f64,
) -> Result<(), String> {
    for app in apps {
        for (with, reason) in &app.accepted {
            if reason.trim().is_empty() {
                return Err(format!(
                    "{}: accepted_hue_overlaps entry for {with:?} has an empty reason",
                    app.name
                ));
            }
        }
    }

    for app in apps {
        for &(status_name, status_hue) in statuses {
            let d = circular_diff(app.accent_hue, status_hue);
            if d <= accent_exclusion_deg && !accepted_for(app, status_name) {
                return Err(format!(
                    "{name}: accent_hue {hue} is {d:.1} degrees from status {status_name} ({status_hue}), at or under accent_exclusion_deg ({accent_exclusion_deg}); add an accepted_hue_overlaps entry with a reason, or change the app's hue",
                    name = app.name,
                    hue = app.accent_hue,
                ));
            }
        }
    }

    for i in 0..apps.len() {
        if apps[i].name == "gallery-proof" {
            continue;
        }
        for j in (i + 1)..apps.len() {
            if apps[j].name == "gallery-proof" {
                continue;
            }
            let d = circular_diff(apps[i].accent_hue, apps[j].accent_hue);
            if d < app_to_app_deg
                && !accepted_for(&apps[i], &apps[j].name)
                && !accepted_for(&apps[j], &apps[i].name)
            {
                return Err(format!(
                    "{a} and {b}: accent hues {ah} and {bh} are {d:.1} degrees apart, under the app-to-app threshold ({app_to_app_deg}); add an accepted_hue_overlaps entry on either app with a reason, or change one app's hue",
                    a = apps[i].name,
                    b = apps[j].name,
                    ah = apps[i].accent_hue,
                    bh = apps[j].accent_hue,
                ));
            }
        }
    }

    Ok(())
}

/// The `design-tokens hues` table: every app accent, every status hue, the
/// dataviz anchors, and every pairwise distance (app-to-app distances are
/// always reported here even when accepted or above threshold — Michal
/// reads this before picking a new app's hue).
pub fn render_hues_table(
    apps: &[AppHue],
    statuses: &[(&str, f64)],
    anchor_hues: &[f64],
    spare_hue: f64,
    accent_exclusion_deg: f64,
    app_to_app_deg: f64,
) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Kaliber hue distances (accent_exclusion_deg={accent_exclusion_deg}, app-to-app threshold={app_to_app_deg}, gallery-proof excluded from app-to-app)\n\nApps:\n"
    ));
    for app in apps {
        out.push_str(&format!("  {:<14} {:>5.1}\n", app.name, app.accent_hue));
    }
    out.push_str("\nStatus hues:\n");
    for (name, hue) in statuses {
        out.push_str(&format!("  {name:<14} {hue:>5.1}\n"));
    }
    out.push_str(&format!("\nDataviz anchors: {anchor_hues:?} (spare {spare_hue})\n"));

    out.push_str("\nApp vs status (degrees; * = at/under accent_exclusion_deg):\n");
    for app in apps {
        for &(status_name, status_hue) in statuses {
            let d = circular_diff(app.accent_hue, status_hue);
            let flag = if d <= accent_exclusion_deg { " *" } else { "" };
            out.push_str(&format!("  {:<14} vs {:<8} {:>6.1}{flag}\n", app.name, status_name, d));
        }
    }

    out.push_str("\nApp vs app (degrees; gallery-proof excluded; * = under app-to-app threshold):\n");
    for i in 0..apps.len() {
        if apps[i].name == "gallery-proof" {
            continue;
        }
        for j in (i + 1)..apps.len() {
            if apps[j].name == "gallery-proof" {
                continue;
            }
            let d = circular_diff(apps[i].accent_hue, apps[j].accent_hue);
            let flag = if d < app_to_app_deg { " *" } else { "" };
            out.push_str(&format!("  {:<14} vs {:<14} {:>6.1}{flag}\n", apps[i].name, apps[j].name, d));
        }
    }

    out
}
