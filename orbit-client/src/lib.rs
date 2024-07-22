use async_fn_stream::try_fn_stream;
use futures::{stream::StreamExt, Stream};
use orbit_core::{ErrorResponse, Progress};
use reqwest::{Response, StatusCode};
use reqwest_eventsource::{Event, RequestBuilderExt};
use url::Url;

pub struct Client {
	url: Url,
	client: reqwest::Client,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error(transparent)]
	Stream(#[from] reqwest_eventsource::Error),

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
	pub fn new(base_url: Url) -> Self {
		Self {
			url: base_url,
			client: reqwest::Client::new(),
		}
	}

	pub async fn deploy(
		&self,
		name: &str,
		r#ref: Option<&str>,
	) -> impl Stream<Item = Result<Result<Progress, orbit_core::Error>, Error>> {
		let mut stream = self
			.client
			.post(self.url.join(&format!("/sites/{name}/deploy")).unwrap())
			.query(&[("ref", r#ref)])
			.eventsource()
			.unwrap();

		try_fn_stream(|emitter| async move {
			while let Some(event) = stream.next().await {
				let event = match event {
					Ok(Event::Open) => continue,
					Ok(Event::Message(message)) => message,
					Err(reqwest_eventsource::Error::InvalidStatusCode(status_code, response)) => {
						if status_code == 404 {
							return Err(Error::SiteNotFound);
						}

						return Err(Error::InvalidResponse(status_code, response));
					},
					Err(reqwest_eventsource::Error::StreamEnded) => return Ok(()),
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
