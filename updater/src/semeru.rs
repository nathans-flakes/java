use std::collections::BTreeMap;

use color_eyre::{
    eyre::{eyre, Context, Result},
    Help, SectionExt,
};
use surf::Client;

use crate::adoptium::{AvailableReleases, Release, ReleaseQuery};

/// Page size
pub const PAGE_SIZE: u64 = 10;

/// Attempts to get the available releases
pub async fn get_available_releases(client: &Client) -> Result<AvailableReleases> {
    let endpoint = "https://api.adoptopenjdk.net/v3/info/available_releases?jvm_impl=openj9";
    client
        .get(endpoint)
        .recv_json()
        .await
        .map_err(|e| eyre!(e))
        .context("Failed to request available versions from semeru")
        .with_section(|| endpoint.to_string().header("Failed Request:"))
}

/// Attempts to get the release info for a particular version
pub async fn get_release(client: &Client, version: u64, release_type: &str) -> Result<Release> {
    let endpoint = format!(
        "https://api.adoptopenjdk.net/v3/assets/feature_releases/{}/{}",
        version, release_type
    );
    let request = client
        .get(endpoint)
        .query(&ReleaseQuery {
            architecture: "x64".to_string(),
            heap_size: "normal".to_string(),
            image_type: "jdk".to_string(),
            os: "linux".to_string(),
            page_size: PAGE_SIZE,
            project: "jdk".to_string(),
            jvm_impl: "openj9".to_string(),
        })
        .map_err(|e| eyre!(e))
        .context("Failed to build request")?
        .build();
    let query = request.url().as_str().to_string();
    let mut releases: Vec<Release> = client
        .recv_json(request)
        .await
        .map_err(|e| eyre!(e))
        .context("Failed to get release information from semeru")
        .with_section(move || query.header("Failed Request"))?;
    releases.sort();
    match releases.pop() {
        Some(release) => Ok(release),
        None => Err(eyre!("Semeru endpoint did not return any valid releases")),
    }
}

/// Attempts to get all the versions
pub async fn get_releases(client: &Client) -> Result<BTreeMap<u64, Release>> {
    let available = get_available_releases(client)
        .await
        .context("Failed to list semeru releases")?;
    let mut output = BTreeMap::new();
    // Get the generally available version of all the available releases
    for version in available.available_releases {
        let release = get_release(client, version, "ga").await.with_context(|| {
            format!("Failed to get version {} from the semeru archive", version)
        })?;
        output.insert(version, release);
    }
    // See if we already have the latest version
    if output.contains_key(&available.most_recent_feature_version) {
        // Go ahead and return
        Ok(output)
    } else {
        let version = available.most_recent_feature_version;
        // Otherwise try to get an EA version of it

        match get_release(client, version, "ea").await {
            Ok(release) => {
                output.insert(version, release);
            }
            Err(e) => {
                eprintln!(
                    "Failed to get version {} (latest) from the semeru archive: {:?}",
                    version, e
                )
            }
        }

        Ok(output)
    }
}
