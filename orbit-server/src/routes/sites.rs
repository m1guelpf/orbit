use crate::{
	config::SiteCollectionExt,
	site::{DeploymentError, DeploymentStage},
};
use aide::axum::{routing::post, ApiRouter};

use axum::{
	extract::{Path, Query},
	http::StatusCode,
	response::sse::{Event, KeepAlive},
	Extension,
};
use futures_util::{stream::Stream, StreamExt};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{config::Sites, misc::Sse};

pub fn handler() -> ApiRouter {
	ApiRouter::new().api_route("/sites/:site/deploy", post(deploy_site))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DeployConfig {
	/// The Git reference to deploy. If not provided, the default branch will be used.
	r#ref: Option<String>,
}

pub async fn deploy_site(
	Path(site_id): Path<String>,
	Query(config): Query<DeployConfig>,
	Extension(sites): Extension<Sites>,
) -> Result<Sse<impl Stream<Item = Result<Event, DeploymentError>>>, StatusCode> {
	let Some(site) = sites.find(&site_id) else {
		return Err(StatusCode::NOT_FOUND);
	};

	let stream = site
		.deploy(config.r#ref)
		.expect("Missing GitHub token")
		.stream()
		.map(|result| {
			let stage = match result {
				Ok(stage) => stage,
				Err(e) => {
					tracing::error!(e = ?e);
					return Event::default().id("error").data(e.to_string());
				},
			};

			let message = match stage {
				DeploymentStage::Deployed => "Deployed site.".to_string(),
				DeploymentStage::Starting => "Starting deployment...".to_string(),
				DeploymentStage::Downloaded => "Downloaded repository.".to_string(),
			};

			Event::default().id("stage").data(message)
		})
		.map(Ok);

	Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
