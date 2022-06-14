use std::{collections::HashMap, process::Command};

use color_eyre::{
    eyre::{eyre, Context, Result},
    Section, SectionExt,
};
use serde::{Deserialize, Serialize};
use surf::Client;

/// Adoptium API
pub mod adoptium;
/// Semeru API
pub mod semeru;

/// Java release struct
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Release {
    link: String,
    major_version: u64,
    java_version: String,
    early_access: bool,
    sha256: String,
}

/// Sources serialization struct
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct Sources {
    versions: HashMap<String, Release>,
    latest: Release,
    stable: Release,
    lts: Release,
}

/// System serialization struct
#[derive(Deserialize, Serialize, Debug, Clone, Default)]
pub struct System {
    temurin: Sources,
    semeru: Sources,
}

impl TryFrom<adoptium::Release> for Release {
    type Error = color_eyre::eyre::Report;

    fn try_from(value: adoptium::Release) -> Result<Self> {
        if value.binaries.len() == 1 {
            let package = &value.binaries[0].package;
            Ok(Release {
                link: package.link.clone(),
                major_version: value.version_data.major,
                java_version: value.version_data.openjdk_version,
                early_access: value.release_type == "ea",
                sha256: get_sha256(&package.link).context("Failed to prefetch package")?,
            })
        } else {
            Err(eyre!(
                "Adoptium release had an incorrect number of binaries"
            ))
        }
    }
}

#[async_std::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    // Create a client
    let client = Client::new();
    // Get list of releases from adoptium, we'll use this for some other things
    let available = adoptium::get_available_releases(&client)
        .await
        .context("Failed to get list of available releases")?;
    let lts_version = available
        .available_lts_releases
        .iter()
        .copied()
        .max()
        .expect("No LTSs?");
    // Get adoptium releases
    let adoptium_releases = get_adoptium_releases(&client).await?;
    // Spit out to the serialization format
    let temurin = Sources {
        versions: adoptium_releases
            .clone()
            .into_iter()
            .map(|(k, v)| (format!("jdk{}", k), v))
            .collect(),
        latest: adoptium_releases
            .get(&available.most_recent_feature_version)
            .expect("Missing release")
            .clone(),
        stable: adoptium_releases
            .get(&available.most_recent_feature_release)
            .expect("Missing release")
            .clone(),
        lts: adoptium_releases
            .get(&lts_version)
            .expect("Missing release")
            .clone(),
    };
    // Get semeru releases

    let semeru_releases = get_semeru_releases(&client).await?;
    // Spit out to the serialization format
    let semeru = Sources {
        versions: semeru_releases
            .clone()
            .into_iter()
            .map(|(k, v)| (format!("jdk{}", k), v))
            .collect(),
        latest: semeru_releases
            .get(&available.most_recent_feature_release)
            .expect("Missing release")
            .clone(),
        stable: semeru_releases
            .get(&available.most_recent_feature_release)
            .expect("Missing release")
            .clone(),
        lts: semeru_releases
            .get(&lts_version)
            .expect("Missing release")
            .clone(),
    };
    let system = System { temurin, semeru };
    let mut systems = HashMap::new();
    systems.insert("x86_64-linux".to_string(), system);
    let output = serde_json::to_string_pretty(&systems).context("Failed to encode sources")?;
    println!("{}", output);
    Ok(())
}

/// Get the releases from adoptium
pub async fn get_adoptium_releases(client: &Client) -> Result<HashMap<u64, Release>> {
    let releases: Result<HashMap<u64, Release>> = adoptium::get_releases(client)
        .await?
        .into_iter()
        .map(|(key, val)| match val.try_into() {
            Ok(val) => Ok((key, val)),
            Err(err) => Err(err),
        })
        .collect();

    releases.context("Failed getting release from adoptium")
}

/// Get the releases from semeru
pub async fn get_semeru_releases(client: &Client) -> Result<HashMap<u64, Release>> {
    let releases: Result<HashMap<u64, Release>> = semeru::get_releases(client)
        .await?
        .into_iter()
        .map(|(key, val)| match val.try_into() {
            Ok(val) => Ok((key, val)),
            Err(err) => Err(err),
        })
        .collect();

    releases.context("Failed getting release from adoptium")
}

/// Gets the nix sha256 for a url
fn get_sha256(url: &str) -> Result<String> {
    let output = Command::new("nix-prefetch-url")
        .args([url, "--type", "sha256"])
        .output()
        .with_section(|| format!("Failed to prefetch url: {}", url).header("Prefetch Failure"))
        .context("Failed to prefetch")?;
    let output = String::from_utf8(output.stdout).context("Invalid utf-8 from nix pre fetch")?;
    // Trim the trailing new line
    Ok(output.trim().to_string())
}
