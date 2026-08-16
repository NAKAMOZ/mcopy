//! Checking GitHub for a newer mcopy, and installing it when the user agrees.
//!
//! Three rules shape this module:
//!
//! - **It never blocks the work.** The check runs only on a bare `mcopy`
//!   launch, never from `copy` or `paste` — those are invoked once per selected
//!   item by the file manager and must not wait on the network.
//! - **It is quiet.** At most one request a day (see [`cache`]), and nothing is
//!   shown unless there is genuinely a newer version.
//! - **It verifies before it executes.** The installers are unsigned, so the
//!   download is checked against the release's `SHA256SUMS` before anything is
//!   run or overwritten.

mod asset;
mod cache;
mod github;
mod installer;

use crate::{log_info, log_warn};

pub use asset::UpdateStyle;
pub use installer::open_releases_page;

/// A release newer than the running build.
#[derive(Clone, Debug)]
pub struct UpdateInfo {
    /// The new version.
    pub version: semver::Version,
    /// The version running now, for the prompt's "x → y" line.
    pub current: semver::Version,
    /// How this build can be updated.
    pub style: UpdateStyle,
    /// The artifact for this platform. Absent for [`UpdateStyle::Manual`].
    asset: Option<github::Asset>,
    /// The release's checksum manifest.
    checksums: Option<github::Asset>,
}

/// Check for a newer release, subject to the daily throttle.
///
/// Returns `None` whenever there is nothing to show the user: not due yet,
/// offline, already current, or a release that carries no artifact this
/// platform can use. Every failure is logged and swallowed — an update check
/// must never be the reason mcopy fails to start.
pub async fn check_for_update() -> Option<UpdateInfo> {
    if !cache::is_due() {
        return None;
    }

    // Recorded before the request: a server that hangs until the timeout must
    // not leave every later launch retrying it.
    cache::record_checked_now();

    let current = match semver::Version::parse(crate::platform::CURRENT_VERSION)
    {
        Ok(current) => current,
        Err(error) => {
            log_warn!("could not parse the running version: {error}");
            return None;
        },
    };

    let client = github::client().ok()?;
    let release = match github::fetch_latest_release(&client).await {
        Ok(release) => release,
        Err(error) => {
            log_warn!("update check failed: {error}");
            return None;
        },
    };

    if release.version <= current {
        log_info!("already at the latest version ({current})");
        return None;
    }

    let style = asset::update_style();
    let picked = asset::pick_for_this_platform(&release.assets).cloned();
    let checksums = release
        .assets
        .iter()
        .find(|candidate| candidate.name == asset::CHECKSUM_ASSET)
        .cloned();

    if style == UpdateStyle::Automatic {
        // Refusing to offer an update we cannot verify is deliberate: these
        // installers are unsigned, and the prompt's whole promise is that
        // pressing the button is safe. A release predating SHA256SUMS, or one
        // missing this platform's artifact, is left to the manual path.
        if picked.is_none() || checksums.is_none() {
            log_warn!(
                "release {} has no verifiable artifact for this platform",
                release.version
            );
            return Some(UpdateInfo {
                version: release.version,
                current,
                style: UpdateStyle::Manual,
                asset: None,
                checksums: None,
            });
        }
    }

    Some(UpdateInfo {
        version: release.version,
        current,
        style,
        asset: picked,
        checksums,
    })
}

/// Download the update, verify it, and hand it to the OS installer.
///
/// Only meaningful for [`UpdateStyle::Automatic`]; the manual path opens the
/// releases page instead.
pub async fn download_and_install(
    info: &UpdateInfo,
) -> anyhow::Result<Outcome> {
    use installer::{UpdateInstaller, Updater};

    let asset = info
        .asset
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("no artifact for this platform"))?;
    let checksums = info.checksums.as_ref().ok_or_else(|| {
        anyhow::anyhow!("this release publishes no checksums")
    })?;

    let client = github::client()?;
    let destination = Updater::download_path(&asset.name)?;

    download_to(&client, &asset.url, &destination).await?;

    if let Err(error) =
        verify(&client, checksums, &asset.name, &destination).await
    {
        // A file that failed verification must not be left where anything
        // could run it later.
        let _ = std::fs::remove_file(&destination);
        return Err(error);
    }

    Updater::install(&destination)?;

    Ok(Outcome {
        message: Updater::completion_message(),
        should_exit: Updater::should_exit_after_install(),
    })
}

/// What the caller should do once an install has been handed off.
pub struct Outcome {
    pub message: &'static str,
    pub should_exit: bool,
}

/// Stream a URL to disk.
///
/// Streamed rather than buffered because these artifacts are ~9 MB and there is
/// no reason to hold one in memory.
async fn download_to(
    client: &reqwest::Client,
    url: &str,
    destination: &std::path::Path,
) -> anyhow::Result<()> {
    use futures::StreamExt;
    use std::io::Write;

    let response = client.get(url).send().await?.error_for_status()?;
    let mut stream = response.bytes_stream();
    let mut file = std::fs::File::create(destination)?;

    while let Some(chunk) = stream.next().await {
        file.write_all(&chunk?)?;
    }
    file.flush()?;

    Ok(())
}

/// Confirm the download matches the digest the release published.
async fn verify(
    client: &reqwest::Client,
    checksums: &github::Asset,
    asset_name: &str,
    downloaded: &std::path::Path,
) -> anyhow::Result<()> {
    let manifest = client
        .get(&checksums.url)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let expected =
        asset::expected_digest(&manifest, asset_name).ok_or_else(|| {
            anyhow::anyhow!("{asset_name} is not listed in {}", checksums.name)
        })?;

    let actual = asset::digest_of(downloaded)?;
    if actual != expected {
        anyhow::bail!(
            "the download did not match its published checksum and was discarded"
        );
    }

    log_info!("verified {asset_name} against {}", checksums.name);
    Ok(())
}

/// Exposed for the prompt window, which shows the file it is about to fetch.
impl UpdateInfo {
    pub fn asset_name(&self) -> Option<&str> {
        self.asset.as_ref().map(|asset| asset.name.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(style: UpdateStyle) -> UpdateInfo {
        UpdateInfo {
            version: semver::Version::new(0, 4, 0),
            current: semver::Version::new(0, 3, 1),
            style,
            asset: None,
            checksums: None,
        }
    }

    /// The whole point of the check: a strictly newer version is an update, an
    /// equal or older one is not. A string compare would get 0.3.10 wrong.
    #[test]
    fn versions_compare_numerically() {
        let current = semver::Version::parse("0.3.9").unwrap();
        let newer = semver::Version::parse("0.3.10").unwrap();
        assert!(newer > current);
        assert!(current <= current.clone());
    }

    /// An automatic update with nothing to download must not be offered as one
    /// — pressing the button would fail with no artifact to fetch.
    #[test]
    fn an_automatic_update_without_an_asset_cannot_install() {
        let info = info(UpdateStyle::Automatic);
        assert!(info.asset_name().is_none());
    }

    #[test]
    fn a_manual_update_has_no_asset() {
        let info = info(UpdateStyle::Manual);
        assert_eq!(info.style, UpdateStyle::Manual);
        assert!(info.asset_name().is_none());
    }
}
