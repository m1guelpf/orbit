use async_fn_stream::try_fn_stream;
use flate2::read::GzDecoder;
use futures_util::{Stream, StreamExt, TryStreamExt};
use http::header;
use orbit_types::{Log, Progress, Stage};
use shlex::Shlex;
use std::{env, fs, path::PathBuf};
use tokio::process::Command;
use uuid::Uuid;

use crate::{
	config::Site,
	misc::{spawn_with_logs, untar_to},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("Failed to bootstrap the project.")]
	Bootstrap(std::io::Error),

	#[error("Failed to clone the repository.")]
	Download(#[from] reqwest::Error),

	#[error("Failed to extract the repository contents.")]
	Extraction(std::io::Error),

	#[error("Failed to configure the deployment.")]
	Configure(std::io::Error),

	#[error("Failed to install dependencies.")]
	InstallDeps(std::io::Error),

	#[error("Failed to run defined commands.")]
	RunCommands(std::io::Error),

	#[error("Failed to optimize the deployment.")]
	Optimize(std::io::Error),

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
			Error::Optimize(_) => Self::Optimize,
			Error::Bootstrap(_) => Self::Bootstrap,
			Error::Configure(_) => Self::Configure,
			Error::Extraction(_) => Self::Extraction,
			Error::InstallDeps(_) => Self::InstallDeps,
			Error::RunCommands(_) => Self::RunCommands,
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

			self.bootstrap_site()?;
			self.download_repo().await?;

			stream.emit(Stage::Downloaded.into()).await;

			self.configure_deployment()?;

			if self.should_install_deps() {
				self.install_deps()
					.try_for_each(|log| async {
						stream.emit(Progress::Log(log)).await;
						Ok(())
					})
					.await?;

				stream.emit(Stage::DepsInstalled.into()).await;
			}

			self.run_commands()
				.try_for_each(|log| async {
					stream.emit(Progress::Log(log)).await;
					Ok(())
				})
				.await?;

			self.optimize_deployment()
				.try_for_each(|log| async {
					stream.emit(Progress::Log(log)).await;
					Ok(())
				})
				.await?;

			stream.emit(Stage::Optimized.into()).await;

			self.migrate()
				.try_for_each(|log| async {
					stream.emit(Progress::Log(log)).await;
					Ok(())
				})
				.await?;

			stream.emit(Stage::Migrated.into()).await;

			self.set_live()?;
			stream.emit(Stage::Deployed.into()).await;

			self.clear_old_deployments()?;

			Ok(())
		})
	}

	fn bootstrap_site(&self) -> Result<(), Error> {
		fs::create_dir_all(self.get_path()).map_err(Error::Bootstrap)?;

		let current_path = self.site.path.join("current");
		if current_path.exists() && !current_path.is_symlink() {
			fs::remove_dir_all(current_path).map_err(Error::Bootstrap)?;
		}

		let storage_path = self.site.path.join("storage");
		if !storage_path.exists() {
			fs::create_dir_all(storage_path.join("logs")).map_err(Error::Bootstrap)?;
			fs::create_dir_all(storage_path.join("app/public")).map_err(Error::Bootstrap)?;
			fs::create_dir_all(storage_path.join("framework/cache")).map_err(Error::Bootstrap)?;
			fs::create_dir_all(storage_path.join("framework/views")).map_err(Error::Bootstrap)?;
			fs::create_dir_all(storage_path.join("framework/sessions"))
				.map_err(Error::Bootstrap)?;
		}

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
			.error_for_status()?
			.bytes()
			.await?;

		untar_to(
			tar::Archive::new(GzDecoder::new(tarball.as_ref())),
			&self.get_path(),
		)
		.map_err(Error::Extraction)?;

		Ok(())
	}

	fn configure_deployment(&self) -> Result<(), Error> {
		let env_path = self.site.path.join(".env");
		if env_path.exists() {
			symlink::symlink_dir(env_path, self.get_path().join(".env"))
				.map_err(Error::Configure)?;
		}

		let storage_path = self.get_path().join("storage");
		if storage_path.exists() {
			fs::remove_dir_all(&storage_path).map_err(Error::Configure)?;
		}
		symlink::symlink_dir(self.site.path.join("storage"), storage_path)
			.map_err(Error::Configure)?;

		Ok(())
	}

	fn install_deps(&self) -> impl Stream<Item = Result<Log, Error>> {
		spawn_with_logs(
			Command::new("composer")
				.arg("install")
				.arg("--no-dev")
				.arg("--prefer-dist")
				.arg("--no-interaction")
				.arg("--optimize-autoloader")
				.current_dir(self.get_path()),
		)
		.map_err(Error::InstallDeps)
	}

	fn run_commands(&self) -> impl Stream<Item = Result<Log, Error>> {
		let mut streams = vec![];

		for command in &self.site.commands {
			let mut argv = Shlex::new(command);

			streams.push(spawn_with_logs(
				Command::new(argv.next().unwrap())
					.args(argv)
					.current_dir(self.get_path()),
			));
		}

		futures_util::stream::iter(streams)
			.flatten()
			.map_err(Error::RunCommands)
	}

	fn migrate(&self) -> impl Stream<Item = Result<Log, Error>> {
		spawn_with_logs(
			Command::new("php")
				.arg("artisan")
				.arg("migrate")
				.arg("--force")
				.current_dir(self.get_path()),
		)
		.map_err(Error::InstallDeps)
	}

	fn optimize_deployment(&self) -> impl Stream<Item = Result<Log, Error>> {
		spawn_with_logs(
			Command::new("php")
				.arg("artisan")
				.arg("optimize")
				.current_dir(self.get_path()),
		)
		.map_err(Error::Optimize)
	}

	fn set_live(&self) -> Result<(), Error> {
		let current_deployment = self.site.path.join("current");
		if current_deployment.exists() {
			fs::remove_file(&current_deployment).map_err(Error::Publish)?;
		}

		symlink::symlink_dir(
			format!("deployments/{}", self.deployment_id),
			current_deployment,
		)
		.map_err(Error::Publish)?;

		Ok(())
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

	fn get_path(&self) -> PathBuf {
		self.site
			.path
			.join(format!("deployments/{}", self.deployment_id))
	}

	fn should_install_deps(&self) -> bool {
		let path = self.get_path();

		path.join("composer.json").exists() && !path.join("vendor").exists()
	}
}
