use anyhow::Result;
use axum::Extension;
use serde::{Deserialize, Serialize};
use slug::slugify;
use std::{path::Path, sync::Arc};

use crate::site::Site;

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
