//! Opt-in update check against GitHub Releases.
//!
//! One unauthenticated GET, run only when the user has said yes (Settings /
//! the first-run prompt wire this up). No token, no credentials, no other
//! request. Every failure — offline, DNS failure, rate limiting, a body
//! that is not the JSON we expect — comes back as `Ok(None)` or a typed
//! `UpdateError`; nothing here panics, blocks, or opens a dialog on its
//! own. The caller decides what, if anything, to show.

use std::time::Duration;

/// The one endpoint this module ever calls. Kept as a named constant, not
/// folded into a format string, so the owner/repo it points at stays
/// visible and easy to change.
const RELEASES_URL: &str = "https://api.github.com/repos/SouthCarpet/Alidade/releases/latest";

/// GitHub rejects requests with no `User-Agent` at all; this carries no
/// identifying information beyond the app name and its own version.
const USER_AGENT: &str = concat!("alidade-app/", env!("CARGO_PKG_VERSION"));

/// Generous but bounded: a hung request must not hang the check forever.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// A release the user can choose to download by hand. `check` never
/// downloads or installs it — `url` only ever opens the GitHub release
/// page in the user's browser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub url: String,
    pub notes: String,
}

/// Every way the check can fail to produce a `Release`. Each variant is
/// meant to be shown, if at all, as a single quiet line in Settings — never
/// as a blocking dialog.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// The request itself did not complete: offline, DNS failure, or it
    /// ran past `REQUEST_TIMEOUT`.
    #[error("update check request failed: {0}")]
    Request(#[from] reqwest::Error),
    /// GitHub answered but not with 200 — this is how rate limiting (403,
    /// with the `x-ratelimit-*` headers) surfaces.
    #[error("update check got HTTP {0}")]
    Status(u16),
    /// The 200 body was not valid JSON.
    #[error("update check could not read GitHub's response")]
    MalformedResponse,
}

/// The subset of GitHub's release JSON this module reads. Every field is
/// optional on purpose: a response that parses as JSON but is missing or
/// nulls out a field (for example a release published with no tag) is a
/// normal, quiet "nothing to report" — not a `MalformedResponse` error.
/// Only a body that fails to parse as JSON at all reaches that error.
#[derive(serde::Deserialize)]
struct GithubRelease {
    tag_name: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    body: Option<String>,
}

/// Ask GitHub for the latest published release and, if it is newer than
/// `current`, return it. A single unauthenticated GET; no auth header is
/// ever added.
pub async fn check(
    current: &str,
    client: &reqwest::Client,
) -> Result<Option<Release>, UpdateError> {
    let response = client
        .get(RELEASES_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .timeout(REQUEST_TIMEOUT)
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(UpdateError::Status(response.status().as_u16()));
    }

    let release: GithubRelease =
        response.json().await.map_err(|_| UpdateError::MalformedResponse)?;

    let Some(tag) = release.tag_name.filter(|tag| !tag.trim().is_empty()) else {
        // A release with no usable tag can't be compared or linked to.
        return Ok(None);
    };

    if !is_newer(current, &tag) {
        return Ok(None);
    }

    Ok(Some(Release {
        url: release.html_url.unwrap_or_default(),
        notes: release.body.unwrap_or_default(),
        tag,
    }))
}

/// `(major, minor, patch)`.
type Core = (u64, u64, u64);

/// Parses a version string into its numeric core plus whether it carries a
/// pre-release suffix (`-` and anything after it, e.g. `-rc1`). Tolerant of
/// a leading `v`/`V`. Returns `None` for anything that isn't exactly three
/// dot-separated non-negative integers — that is the one "malformed"
/// signal both callers below rely on.
fn parse_version(input: &str) -> Option<(Core, bool)> {
    let input = input.strip_prefix('v').or_else(|| input.strip_prefix('V')).unwrap_or(input);
    let core_str = match input.split_once('-') {
        Some((core, _prerelease)) => core,
        None => input,
    };
    let mut parts = core_str.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next()?.parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    let is_prerelease = core_str.len() < input.len();
    Some(((major, minor, patch), is_prerelease))
}

/// Is `tag` a release the user should be told about, given they are
/// running `current`? Tolerant of a leading `v`/`V` on either side (GitHub
/// tag convention). A malformed tag (fails to parse into exactly three
/// non-negative integers — `"latest"`, `""`, garbage) is never newer: this
/// backs a purely informational, opt-in prompt, so "no update" is always
/// the safe answer when the input can't be understood. The same applies if
/// `current` itself fails to parse.
///
/// Pre-release decision: a `tag` carrying a pre-release suffix (`0.2.0-rc1`,
/// `v0.2.0-beta.1`, …) is never reported as newer, even when its numeric
/// core is greater than `current`. This check drives a single quiet
/// "update available" prompt aimed at ordinary users, not testers; offering
/// a release candidate through that same prompt would read as "download
/// this, it's the stable release" when it isn't. A pre-release only starts
/// counting once GitHub actually publishes the plain tag it previews.
pub fn is_newer(current: &str, tag: &str) -> bool {
    let Some((current_core, _)) = parse_version(current) else {
        return false;
    };
    let Some((tag_core, tag_is_prerelease)) = parse_version(tag) else {
        return false;
    };
    if tag_is_prerelease {
        return false;
    }
    tag_core > current_core
}
