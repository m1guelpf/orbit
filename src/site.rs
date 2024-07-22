use async_fn_stream::try_fn_stream;
use axum::http::StatusCode;

use flate2::read::GzDecoder;
use futures_util::Stream;
use http::header;
use serde::{Deserialize, Serialize};
use std::{
	env::{self, VarError},
	fs,
	path::PathBuf,
};
use uuid::Uuid;

#[allow(clippy::unsafe_derive_deserialize)]
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Site {
	pub name: String,
	pub path: PathBuf,
	pub github_repo: String,
}

impl Site {
	pub fn deploy(self, r#ref: Option<String>) -> Result<Deployer, VarError> {
		Deployer::from_site(self, r#ref)
	}
}

#[derive(Debug)]
pub enum DeploymentStage {
	Starting,
	Downloaded,
	Deployed,
}

#[derive(Debug, thiserror::Error)]
pub enum DeploymentError {
	#[error("Failed to clone the repository.")]
	Download(#[from] reqwest::Error),

	#[error("Failed to extract the repository contents.")]
	Extraction(#[from] std::io::Error),

	#[error("Failed to cleanup old deployments.")]
	Cleanup(std::io::Error),

	#[error("Failed to publish the new deployment.")]
	Publish(std::io::Error),
}

pub struct Deployer {
	site: Site,
	deployment_id: Uuid,
	github_token: String,
	r#ref: Option<String>,
	client: reqwest::Client,
}

impl Deployer {
	fn from_site(site: Site, r#ref: Option<String>) -> Result<Self, VarError> {
		env::var("GITHUB_TOKEN").map(|github_token| Self {
			site,
			r#ref,
			github_token,
			deployment_id: Uuid::now_v7(),
			client: reqwest::Client::builder()
				.user_agent("orbit-deployer")
				.build()
				.unwrap(),
		})
	}

	pub fn stream(
		self,
	) -> impl Stream<Item = std::result::Result<DeploymentStage, DeploymentError>> {
		try_fn_stream(|stream| async move {
			stream.emit(DeploymentStage::Starting).await;

			self.download_repo().await?;

			stream.emit(DeploymentStage::Downloaded).await;

			// install deps, cache, etc. here

			self.set_live()?;
			self.clear_old_deployments()?;

			stream.emit(DeploymentStage::Deployed).await;

			Ok(())
		})
	}

	fn clear_old_deployments(&self) -> Result<(), DeploymentError> {
		let deployments =
			fs::read_dir(self.site.path.join("deployments")).map_err(DeploymentError::Cleanup)?;

		let mut deployments: Vec<_> = deployments
			.map(|entry| {
				entry
					.and_then(|e| {
						let metadata = e.metadata()?;
						Ok((e.path(), metadata.modified()?))
					})
					.map_err(DeploymentError::Cleanup)
			})
			.collect::<Result<_, DeploymentError>>()?;

		deployments.sort_by_key(|(_, modified)| *modified);
		deployments
			.into_iter()
			.filter(|(path, _)| path.is_dir() && path != &self.get_path())
			.rev()
			.skip(2)
			.try_for_each(|(path, _)| fs::remove_dir_all(path))
			.map_err(DeploymentError::Cleanup)?;

		Ok(())
	}

	async fn download_repo(&self) -> Result<(), DeploymentError> {
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

	fn set_live(&self) -> Result<(), DeploymentError> {
		let current_deployment = self.site.path.join("current");
		if current_deployment.exists() {
			fs::remove_file(&current_deployment).map_err(DeploymentError::Publish)?;
		}

		std::os::unix::fs::symlink(
			format!("deployments/{}", self.deployment_id),
			current_deployment,
		)
		.map_err(DeploymentError::Publish)?;

		Ok(())
	}

	fn get_path(&self) -> PathBuf {
		self.site
			.path
			.join(format!("deployments/{}", self.deployment_id))
	}
}

impl axum::response::IntoResponse for DeploymentError {
	fn into_response(self) -> axum::response::Response {
		(StatusCode::INTERNAL_SERVER_ERROR, self.to_string()).into_response()
	}
}
