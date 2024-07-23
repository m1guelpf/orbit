use async_fn_stream::try_fn_stream;
use flate2::read::GzDecoder;
use futures_util::{Stream, TryStreamExt};
use http::header;
use orbit_types::{Log, Progress, Stage};
use std::{env, fs, path::PathBuf};
use tokio::process::Command;
use uuid::Uuid;

use crate::{config::Site, misc::spawn_with_logs};

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Failed to clone the repository.")]
	Download(#[from] reqwest::Error),

	#[error("Failed to extract the repository contents.")]
	Extraction(#[from] std::io::Error),

	#[error("Failed to install dependencies.")]
	InstallDeps(std::io::Error),

	#[error("Failed to cleanup old deployments.")]
	Cleanup(std::io::Error),

	#[error("Failed to publish the new deployment.")]
	Publish(std::io::Error),
}

impl From<Error> for orbit_types::Error {
	fn from(value: Error) -> Self {
		match value {
			Error::Cleanup(_) => Self::Cleanup,
			Error::Publish(_) => Self::Publish,
			Error::Download(_) => Self::Download,
			Error::Extraction(_) => Self::Extraction,
			Error::InstallDeps(_) => Self::InstallDeps,
		}
	}
}

impl From<Error> for orbit_types::ErrorResponse {
	fn from(value: Error) -> Self {
		Self::from(orbit_types::Error::from(value))
	}
}

pub struct Deployer {
	site: Site,
	deployment_id: Uuid,
	github_token: String,
	r#ref: Option<String>,
	client: reqwest::Client,
}

impl Deployer {
	pub fn from_site(site: Site, r#ref: Option<String>) -> Self {
		// we unwrap here since Config::validate errors ealier if GITHUB_TOKEN is not set
		let github_token = env::var("GITHUB_TOKEN").expect("GITHUB_TOKEN is not set");

		Self {
			site,
			r#ref,
			github_token,
			deployment_id: Uuid::now_v7(),
			client: reqwest::Client::builder()
				.user_agent("orbit-deployer")
				.build()
				.unwrap(),
		}
	}

	pub fn stream(self) -> impl Stream<Item = std::result::Result<Progress, Error>> {
		try_fn_stream(|stream| async move {
			stream.emit(Stage::Starting.into()).await;

			self.download_repo().await?;

			stream.emit(Stage::Downloaded.into()).await;

			self.install_deps()
				.try_for_each(|log| async {
					stream.emit(Progress::Log(log)).await;
					Ok(())
				})
				.await?;

			stream.emit(Stage::DepsInstalled.into()).await;

			// cache, etc. here

			self.set_live()?;
			self.clear_old_deployments()?;

			stream.emit(Stage::Deployed.into()).await;

			Ok(())
		})
	}

	fn clear_old_deployments(&self) -> Result<(), Error> {
		let deployments =
			fs::read_dir(self.site.path.join("deployments")).map_err(Error::Cleanup)?;

		let mut deployments: Vec<_> = deployments
			.map(|entry| {
				entry
					.and_then(|e| {
						let metadata = e.metadata()?;
						Ok((e.path(), metadata.modified()?))
					})
					.map_err(Error::Cleanup)
			})
			.collect::<Result<_, Error>>()?;

		deployments.sort_by_key(|(_, modified)| *modified);
		deployments
			.into_iter()
			.filter(|(path, _)| path.is_dir() && path != &self.get_path())
			.rev()
			.skip(2)
			.try_for_each(|(path, _)| fs::remove_dir_all(path))
			.map_err(Error::Cleanup)?;

		Ok(())
	}

	async fn download_repo(&self) -> Result<(), Error> {
		// we unwrap here since Config::validate errors ealier if `github_repo` does not cointain a `/`
		let (owner, repo) = self.site.github_repo.split_once('/').unwrap();

		let tarball = self
			.client
			.get(format!(
				"https://api.github.com/repos/{owner}/{repo}/tarball/{}",
				self.r#ref
					.as_ref()
					.map_or(String::new(), |r#ref| format!("/{ref}"))
			))
			.header(
				header::AUTHORIZATION,
				format!("Bearer {}", self.github_token),
			)
			.send()
			.await?
			.bytes()
			.await?;

		let path = &self.get_path();
		let mut tar = tar::Archive::new(GzDecoder::new(tarball.as_ref()));

		for entry in tar.entries()? {
			let mut file = entry?;
			let file_path = file.path()?.into_owned();
			let file_path = file_path
				.strip_prefix(file_path.components().next().unwrap())
				.unwrap()
				.to_owned();

			if file_path.to_str() == Some("") {
				continue;
			}

			if !file.header().entry_type().is_dir() {
				fs::create_dir_all(path.join(&file_path).parent().unwrap())?;
				file.unpack(path.join(file_path))?;
			}
		}

		Ok(())
	}

	fn install_deps(&self) -> impl Stream<Item = Result<Log, Error>> {
		spawn_with_logs(
			Command::new("composer")
				.arg("install")
				.current_dir(self.get_path()),
		)
		.map_err(Error::InstallDeps)
	}

	fn set_live(&self) -> Result<(), Error> {
		let current_deployment = self.site.path.join("current");
		if current_deployment.exists() {
			fs::remove_file(&current_deployment).map_err(Error::Publish)?;
		}

		std::os::unix::fs::symlink(
			format!("deployments/{}", self.deployment_id),
			current_deployment,
		)
		.map_err(Error::Publish)?;

		Ok(())
	}

	fn get_path(&self) -> PathBuf {
		self.site
			.path
			.join(format!("deployments/{}", self.deployment_id))
	}
}