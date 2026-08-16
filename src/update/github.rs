//! The GitHub Releases API client.
//!
//! Only one endpoint is used, unauthenticated: the release marked latest.
//! At one request per day the 60/hour anonymous rate limit is irrelevant, and
//! not sending a token keeps mcopy from ever holding a credential.

use std::time::Duration;

/// The `releases/latest` endpoint for this repository.
const API_URL: &str =
    "https://api.github.com/repos/NAKAMOZ/mcopy/releases/latest";

/// A launch must not wait on a slow network. Exceeding this simply means no
/// update prompt this run.
const TIMEOUT: Duration = Duration::from_secs(6);

/// One downloadable file attached to a release.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Asset {
    pub name: String,
    #[serde(rename = "browser_download_url")]
    pub url: String,
}

#[derive(Debug, serde::Deserialize)]
struct RawRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<Asset>,
}

/// The latest published release.
#[derive(Debug)]
pub struct Release {
    pub version: semver::Version,
    pub assets: Vec<Asset>,
}

/// Build a client shared by the release check and the asset download.
///
/// GitHub rejects requests without a `User-Agent`, so it is set once here
/// rather than at each call site.
pub fn client() -> anyhow::Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(TIMEOUT)
        .user_agent(format!("mcopy/{}", crate::platform::CURRENT_VERSION))
        .build()?)
}

/// Fetch and parse the latest release.
pub async fn fetch_latest_release(
    client: &reqwest::Client,
) -> anyhow::Result<Release> {
    let raw: RawRelease = client
        .get(API_URL)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    // Tags are `vX.Y.Z`; semver itself does not accept the prefix.
    let version = semver::Version::parse(raw.tag_name.trim_start_matches('v'))?;

    Ok(Release {
        version,
        assets: raw.assets,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_release_payload_parses() {
        let payload = r#"{
            "tag_name": "v0.4.0",
            "assets": [
                {
                    "name": "mcopy-0.4.0-x86_64.AppImage",
                    "browser_download_url": "https://example.invalid/a.AppImage"
                }
            ]
        }"#;

        let raw: RawRelease = serde_json::from_str(payload).unwrap();
        assert_eq!(raw.tag_name, "v0.4.0");
        assert_eq!(raw.assets.len(), 1);
        assert_eq!(raw.assets[0].name, "mcopy-0.4.0-x86_64.AppImage");
        assert_eq!(raw.assets[0].url, "https://example.invalid/a.AppImage");
    }

    /// A release with no attached files is not an error, just nothing to offer.
    #[test]
    fn a_release_without_assets_parses() {
        let raw: RawRelease =
            serde_json::from_str(r#"{"tag_name": "v0.4.0"}"#).unwrap();
        assert!(raw.assets.is_empty());
    }

    #[test]
    fn the_v_prefix_is_stripped_before_parsing() {
        let version = semver::Version::parse("v0.4.0".trim_start_matches('v'))
            .expect("a tag must parse once the prefix is gone");
        assert_eq!(version, semver::Version::new(0, 4, 0));
    }
}
