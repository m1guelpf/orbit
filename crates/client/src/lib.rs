#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use async_fn_stream::try_fn_stream;
use futures::{stream::StreamExt, Stream};
use orbit_types::{ErrorResponse, Progress};
use reqwest::{header, Response, StatusCode};
use reqwest_eventsource::{Event, RequestBuilderExt};
use url::Url;

pub struct Client {
	base_url: Url,
	token: String,
	client: reqwest::Client,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error(transparent)]
	Stream(#[from] reqwest_eventsource::Error),

	#[error("{0}")]
	Transport(#[from] reqwest::Error),

	#[error("Invalid authentication token")]
	Unauthorized,

	#[error("Could not find the requested site")]
	SiteNotFound,

	#[error("The server returned an invalid response.")]
	InvalidResponse(StatusCode, Response),

	#[error("The server returned an invalid event: {0}")]
	InvalidEvent(String),

	#[error("Could not decode the event data")]
	Decoding(#[from] serde_json::Error),
}

impl Client {
	/// Create a new client.
	#[must_use]
	pub fn new(base_url: Url, token: String) -> Self {
		Self {
			token,
			base_url,
			client: reqwest::Client::new(),
		}
	}

	/// Deploy a site.
	#[allow(clippy::missing_panics_doc)]
	pub fn deploy(
		&self,
		name: &str,
		r#ref: Option<&str>,
	) -> impl Stream<Item = Result<Result<Progress, orbit_types::Error>, Error>> {
		let mut stream = self
			.client
			.post(
				self.base_url
					.join(&format!("/sites/{name}/deploy"))
					.unwrap(),
			)
			.query(&[("ref", r#ref)])
			.header(header::AUTHORIZATION, format!("Bearer {}", self.token))
			.eventsource()
			.unwrap();

		try_fn_stream(|emitter| async move {
			while let Some(event) = stream.next().await {
				let event = match event {
					Ok(Event::Open) => continue,
					Ok(Event::Message(message)) => message,
					Err(reqwest_eventsource::Error::InvalidStatusCode(status_code, response)) => {
						match status_code {
							StatusCode::NOT_FOUND => return Err(Error::SiteNotFound),
							StatusCode::UNAUTHORIZED => return Err(Error::Unauthorized),
							_ => return Err(Error::InvalidResponse(status_code, response)),
						}
					},
					Err(reqwest_eventsource::Error::StreamEnded) => return Ok(()),
					Err(reqwest_eventsource::Error::Transport(err)) => return Err(err.into()),
					Err(err) => return Err(err.into()),
				};

				let response = match event.id.as_ref() {
					"log" => Ok(Progress::Log(serde_json::from_str(&event.data)?)),
					"stage" => Ok(Progress::Stage(serde_json::from_str(&event.data)?)),
					"error" => Err(serde_json::from_str::<ErrorResponse>(&event.data)?.error),
					_ => return Err(Error::InvalidEvent(format!("{}: {}", event.id, event.data))),
				};

				emitter.emit(response).await;
			}

			unreachable!("The stream should not end without a StreamEnded error");
		})
	}
}
