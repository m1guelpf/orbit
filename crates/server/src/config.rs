use anyhow::{bail, Result};
use axum::Extension;
use serde::{Deserialize, Serialize};
use slug::slugify;
use std::{
	path::{Path, PathBuf},
	sync::Arc,
};

use crate::deploy::Deployer;

#[derive(Debug, Serialize, Deserialize)]
pub struct Config {
	version: usize,
	pub sites: Vec<Site>,
}

impl Config {
	/// Load the config from a TOML file at the given path.
	pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
		if !path.as_ref().exists() {
			return Err(anyhow::anyhow!(
				"Could not locate Orbit config file at path {}",
				path.as_ref().display()
			));
		}

		let file = std::fs::read_to_string(path)?;
		let config: Self = toml::from_str(&file)?;

		if config.version != 1 {
			return Err(anyhow::anyhow!("Unsupported version: {}", config.version));
		}

		Ok(config)
	}

	pub fn validate(self) -> Result<Self> {
		if std::env::var("GITHUB_TOKEN").is_err() {
			return Err(anyhow::anyhow!("GITHUB_TOKEN is not set"));
		}

		self.sites.iter().try_for_each(|site| {
			if !site.github_repo.contains('/') {
				bail!(
					"Invalid github_repo for site {}. Must be in the format of owner/repo",
					site.name
				);
			}

			Ok(())
		})?;

		Ok(self)
	}

	pub fn extension(self) -> Extension<Sites> {
		Extension(Arc::new(self.sites))
	}
}

pub type Sites = Arc<Vec<Site>>;

pub trait SiteCollectionExt {
	fn find(&self, slug: &str) -> Option<Site>;
}

impl SiteCollectionExt for Vec<Site> {
	fn find(&self, slug: &str) -> Option<Site> {
		self.iter()
			.find(|site| slugify(&site.name) == slug)
			.cloned()
	}
}

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Site {
	pub name: String,
	pub path: PathBuf,
	pub github_repo: String,
	#[serde(default)]
	pub commands: Vec<String>,
}

impl Site {
	pub fn deploy(self, r#ref: Option<String>) -> Deployer {
		Deployer::from_site(self, r#ref)
	}
}
