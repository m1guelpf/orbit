use std::{convert::Infallible, sync::Arc};

use aide::axum::{routing::post, ApiRouter};
use axum::{
	extract::{Path, Query},
	http::StatusCode,
	response::sse::{Event, KeepAlive},
	Extension,
};
use axum_extra::{
	headers::{authorization::Bearer, Authorization},
	TypedHeader,
};
use futures_util::{stream::Stream, StreamExt};
use orbit_types::{ErrorResponse, Progress};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{
	config::{Config, SiteCollectionExt},
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
	Query(params): Query<DeployConfig>,
	Extension(config): Extension<Arc<Config>>,
	TypedHeader(authorization): TypedHeader<Authorization<Bearer>>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, StatusCode> {
	if authorization.token() != config.token {
		return Err(StatusCode::UNAUTHORIZED);
	}

	let Some(site) = config.sites.find(&site_id) else {
		return Err(StatusCode::NOT_FOUND);
	};

	let stream = site
		.deploy(params.r#ref)
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
