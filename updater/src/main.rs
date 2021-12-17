use std::collections::{HashMap, HashSet};

use color_eyre::{
    eyre::{bail, Context, ContextCompat, Result},
    Help, SectionExt,
};
use serde::{Deserialize, Serialize};

fn main() -> Result<()> {
    color_eyre::install()?;

    // Get the available versions
    let versions = get_all_versions()?;
    // Uniqueify them
    let mut slugs: HashMap<String, (Release, String)> = HashMap::new();
    for version in versions {
        if let Ok(url) = get_url(&version) {
            let slug = version.to_slug();
            let entry = slugs.entry(slug).or_insert((version.clone(), url.clone()));
            if version > entry.0 {
                *entry = (version, url)
            }
        }
    }
    let mut sources = Sources {
        versions: HashMap::new(),
    };
    for (_slug, (version, url)) in slugs {
        sources.add_release(&version, &url);
    }
    let mut output: HashMap<String, Sources> = HashMap::new();
    output.insert("x86_64-unknown-linux-gnu".to_string(), sources);
    let json_output =
        serde_json::to_string_pretty(&output).context("Failed to serialize output")?;
    println!("{}", json_output);

    Ok(())
}

/// Serde serialization format
#[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
struct Sources {
    versions: HashMap<String, Source>,
}
impl Sources {
    fn add_release(&mut self, release: &Release, url: &str) {
        let version = release.to_java_version();
        let source = Source {
            major_version: release.major.to_string(),
            version,
            url: url.to_string(),
        };
        self.versions.insert(release.major.to_string(), source);
    }
}
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct Source {
    major_version: String,
    version: String,
    url: String,
}

/// Serde formatting struct
#[derive(Serialize, Deserialize, PartialEq, Eq, Hash, Debug, Clone)]
struct Release {
    build: usize,
    major: usize,
    minor: usize,
    openjdk_version: String,
    security: usize,
    semver: String,
}

impl PartialOrd for Release {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        match self.build.partial_cmp(&other.build) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.major.partial_cmp(&other.major) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.minor.partial_cmp(&other.minor) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.openjdk_version.partial_cmp(&other.openjdk_version) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.security.partial_cmp(&other.security) {
            Some(core::cmp::Ordering::Equal) => {}
            ord => return ord,
        }
        match self.openjdk_version.partial_cmp(&other.openjdk_version) {
            Some(core::cmp::Ordering::Equal) => Some(core::cmp::Ordering::Equal),
            _ => {
                let x = self.openjdk_version.split('-').collect::<Vec<_>>();
                let x = x[x.len() - 1];
                let y = other.openjdk_version.split('-').collect::<Vec<_>>();
                let y = y[y.len() - 1];
                x.partial_cmp(y)
            }
        }
    }
}

impl Ord for Release {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl Release {
    fn to_slug(&self) -> String {
        format!("1.{}.{}", self.major, self.minor)
    }
    fn to_java_version(&self) -> String {
        format!(
            "{}.{}.{}+{}",
            self.major, self.minor, self.security, self.build
        )
    }
}

/// Serde formatting struct
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct Versions {
    versions: Vec<Release>,
}

/// Get a page of the versions
fn get_versions_page(page: usize) -> Result<Vec<Release>> {
    let page = format!("{}", page);
    let request = ureq::get("https://api.adoptium.net/v3/info/release_versions")
        .query("architecture", "x64")
        .query("image_type", "jdk")
        .query("os", "linux")
        .query("page", &page)
        .query("page_size", "10")
        .query("project", "jdk");
    let url = request.url().to_string();
    let response = request
        .call()
        .with_section(|| format!("Page: {} Url: {}", page, url).header("Failed Request:"))
        .context("Failed getting page of versions list")?;
    let versions: Versions = response
        .into_json()
        .with_section(|| format!("Page: {} Url: {}", page, url).header("Failed Request:"))
        .context("Failed deserializing page of versions list")?;
    Ok(versions.versions)
}

/// Get all the versions available
fn get_all_versions() -> Result<HashSet<Release>> {
    let mut versions = HashSet::new();
    // Go through the pages until we hit a 404
    let mut page_number = 0;
    // Err early if the first page fails, so we can have a more useful context message
    let mut page =
        Ok(get_versions_page(0).context("Failed getting first page in get_all_versions")?);
    while let Ok(current_page) = page {
        for version in current_page {
            versions.insert(version);
        }
        page_number += 1;
        page = get_versions_page(page_number).context("Ran out of pages");
    }
    Ok(versions)
}

/// Serde deserialization type
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct VersionInfo {
    binaries: Vec<BinaryInfo>,
}

/// Serde deserialization type
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct BinaryInfo {
    package: PackageInfo,
}
/// Serde deserialization type
#[derive(Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct PackageInfo {
    link: String,
    name: String,
}

/// Gets the url for the latest release of a version
fn get_url(version: &Release) -> Result<String> {
    let request = ureq::get(&format!(
        "https://api.adoptium.net/v3/assets/version/{}",
        version.openjdk_version
    ))
    .query("architecture", "x64")
    .query("image_type", "jdk")
    .query("os", "linux")
    .query("page", "0")
    .query("page_size", "10")
    .query("project", "jdk");
    let url = request.url().to_string();
    let response = request
        .call()
        .with_section(|| format!("Url: {}", url).header("Failed Request"))
        .context("Failed requesting version information")?;
    let version_info: Vec<VersionInfo> = response
        .into_json()
        .with_section(|| format!("Url: {}", url).header("Failed Request"))
        .context("Failed to deserialize response for version information")?;
    let version_info = version_info
        .into_iter()
        .next()
        .context("Nothing providred")?;

    match version_info.binaries.get(0) {
        Some(binary_info) => Ok(binary_info.package.link.clone()),
        None => bail!("Binary was missing!"),
    }
}
