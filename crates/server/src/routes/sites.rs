use std::convert::Infallible;

use aide::axum::{routing::post, ApiRouter};
use axum::{
	extract::{Path, Query},
	http::StatusCode,
	response::sse::{Event, KeepAlive},
	Extension,
};
use futures_util::{stream::Stream, StreamExt};
use orbit_types::{ErrorResponse, Progress};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
	config::{SiteCollectionExt, Sites},
	misc::Sse,
};

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
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
	let Some(site) = sites.find(&site_id) else {
		return Err(StatusCode::NOT_FOUND);
	};

	let stream = site
		.deploy(config.r#ref)
		.stream()
		.map(|result| match result {
			Ok(Progress::Log(log)) => Event::default().id("log").json_data(log).unwrap(),
			Ok(Progress::Stage(stage)) => Event::default().id("stage").json_data(stage).unwrap(),
			Err(e) => {
				tracing::error!(e = ?e);

				Event::default()
					.id("error")
					.json_data(ErrorResponse::from(e))
					.unwrap()
			},
		})
		.map(Ok);

	Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}
