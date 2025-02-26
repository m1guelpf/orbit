#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use std::env;

use anyhow::Result;
use config::Config;
use dotenvy::dotenv;
use tracing_subscriber::{
	prelude::__tracing_subscriber_SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

mod config;
mod deploy;
mod misc;
mod routes;
mod server;

#[tokio::main]
async fn main() -> Result<()> {
	dotenv().ok();

	tracing_subscriber::registry()
		.with(tracing_subscriber::fmt::layer().with_filter(
			EnvFilter::try_from_default_env().unwrap_or_else(|_| "orbit_server=info".into()),
		))
		.init();

	let config =
		Config::load(env::var("ORBIT_CONFIG").expect("$ORBIT_CONFIG not found"))?.validate()?;

	server::start(config).await
}
