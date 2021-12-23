use std::collections::HashMap;

use color_eyre::{
    eyre::{eyre, Context, Result},
    Help, SectionExt,
};
use serde::{Deserialize, Serialize};
use surf::Client;

/// Page size
pub const PAGE_SIZE: u64 = 10;

/// Response from `/v3/info/available_releases` endpoint
#[derive(Deserialize, Serialize, Debug)]
pub struct AvailableReleases {
    pub available_lts_releases: Vec<u64>,
    pub available_releases: Vec<u64>,
    pub most_recent_feature_release: u64,
    pub most_recent_feature_version: u64,
    pub tip_version: u64,
}

/// Package for a particular binary
#[derive(Deserialize, Serialize, Debug)]
pub struct Package {
    checksum: String,
    checksum_link: String,
    download_count: u64,
    pub link: String,
    metadata_link: String,
    name: String,
    size: u64,
}

/// Information about a particular binary
#[derive(Deserialize, Serialize, Debug)]
pub struct Binary {
    architecture: String,
    download_count: u64,
    heap_size: String,
    image_type: String,
    jvm_impl: String,
    os: String,
    pub package: Package,
    project: String,
    scm_ref: String,
    updated_at: String,
}

/// Information about a source
#[derive(Deserialize, Serialize, Debug)]
pub struct Source {
    link: String,
    name: String,
    size: u64,
}

/// Version data
#[derive(Deserialize, Serialize, Debug, PartialEq, Eq)]
pub struct VersionData {
    pub build: u64,
    pub major: u64,
    pub minor: u64,
    pub openjdk_version: String,
    pub security: u64,
    pub semver: String,
}

impl PartialOrd for VersionData {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.major.partial_cmp(&other.major) {
            Some(std::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.minor.partial_cmp(&other.minor) {
            Some(std::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.security.partial_cmp(&other.security) {
            Some(std::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        self.build.partial_cmp(&other.build)
    }
}

impl Ord for VersionData {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

/// Information about a particular feature release
#[derive(Deserialize, Serialize, Debug)]
pub struct Release {
    pub binaries: Vec<Binary>,
    download_count: u64,
    id: String,
    release_link: String,
    pub release_type: String,
    source: Option<Source>,
    timestamp: String,
    updated_at: String,
    vendor: String,
    pub version_data: VersionData,
}

impl PartialEq for Release {
    fn eq(&self, other: &Self) -> bool {
        self.version_data == other.version_data
    }
}

impl Eq for Release {}

impl PartialOrd for Release {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.version_data.partial_cmp(&other.version_data)
    }
}

impl Ord for Release {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.version_data.cmp(&other.version_data)
    }
}

/// Attempts to get the available releases
pub async fn get_available_releases(client: &Client) -> Result<AvailableReleases> {
    let endpoint = "https://api.adoptium.net/v3/info/available_releases";
    client
        .get(endpoint)
        .recv_json()
        .await
        .map_err(|e| eyre!(e))
        .context("Failed to request available versions from adoptium")
        .with_section(|| endpoint.to_string().header("Failed Request:"))
}

/// Release query struct
#[derive(Deserialize, Serialize, Debug)]
pub struct ReleaseQuery {
    architecture: String,
    heap_size: String,
    image_type: String,
    os: String,
    page_size: u64,
    project: String,
}

/// Attempts to get the release info for a particular version
pub async fn get_release(client: &Client, version: u64, release_type: &str) -> Result<Release> {
    let endpoint = format!(
        "https://api.adoptium.net/v3/assets/feature_releases/{}/{}",
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
        })
        .map_err(|e| eyre!(e))
        .context("Failed to build request")?
        .build();
    let query = request.url().as_str().to_string();
    let mut releases: Vec<Release> = client
        .recv_json(request)
        .await
        .map_err(|e| eyre!(e))
        .context("Failed to get release information from adoptium")
        .with_section(move || query.header("Failed Request"))?;
    releases.sort();
    match releases.pop() {
        Some(release) => Ok(release),
        None => Err(eyre!("Adoptium endpoint did not return any valid releases")),
    }
}

/// Attempts to get all the versions
pub async fn get_releases(client: &Client) -> Result<HashMap<u64, Release>> {
    let available = get_available_releases(client)
        .await
        .context("Failed to list adoptium releases")?;
    let mut output = HashMap::new();
    // Get the generally available version of all the available releases
    for version in available.available_releases {
        let release = get_release(client, version, "ga").await.with_context(|| {
            format!(
                "Failed to get version {} from the adoptium archive",
                version
            )
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
        let release = get_release(client, version, "ea").await.with_context(|| {
            format!(
                "Failed to get version {} (latest) from the adoptium archive",
                version
            )
        })?;
        output.insert(version, release);
        Ok(output)
    }
}
