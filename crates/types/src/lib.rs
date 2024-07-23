#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use serde::{Deserialize, Serialize};

/// A progress update for a deployment.
pub enum Progress {
	Log(Log),
	Stage(Stage),
}

/// A log message.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "log")]
pub enum Log {
	Info(String),
	Error(String),
}

/// The stage of the deployment.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stage {
	/// The deployment has been started.
	Starting,
	/// The current deployment has been downloaded.
	Downloaded,
	/// Dependencies for the current deployment have been installed.
	DepsInstalled,
	/// The deployment is now live.
	Deployed,
}

#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Error {
	/// Failed to clone the repository.
	#[error("Failed to clone the repository.")]
	Download,

	/// Failed to extract the repository contents.
	#[error("Failed to extract the repository contents.")]
	Extraction,

	/// Failed to install dependencies.
	#[error("Failed to install dependencies.")]
	InstallDeps,

	/// Failed to build the deployment.
	#[error("Failed to cleanup old deployments.")]
	Cleanup,

	/// Failed to build the deployment.
	#[error("Failed to publish the new deployment.")]
	Publish,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
	pub error: Error,
	pub message: String,
}

impl From<Error> for ErrorResponse {
	fn from(error: Error) -> Self {
		Self {
			message: error.to_string(),
			error,
		}
	}
}

impl From<Stage> for Progress {
	fn from(value: Stage) -> Self {
		Self::Stage(value)
	}
}
