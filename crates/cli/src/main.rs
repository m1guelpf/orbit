#![warn(clippy::all, clippy::pedantic, clippy::nursery)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use orbit_client::Client;
use orbit_types::{Log, Progress, Stage};
use pin_utils::pin_mut;
use url::Url;

mod utils;

#[derive(Debug, Parser)]
#[clap(
	name = "orbit",
	about = "🪐 Trigger Orbit deploys from the command line.",
	version,
	author
)]
struct Cli {
	/// URL to the Orbit instance.
	#[arg(short, long, env = "ORBIT_URL")]
	url: Url,

	/// Orbit authenticaton token.
	#[arg(short, long, env = "ORBIT_TOKEN")]
	token: String,

	/// Enable debug mode
	#[clap(short = 'D', long)]
	pub debug: bool,

	#[clap(subcommand)]
	command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
	/// Trigger a deploy for an Orbit site.
	Deploy {
		/// The name of the site to deploy.
		slug: String,

		/// The git ref to deploy. If not provided, the default branch will be used.
		#[arg(long, env = "DEPLOY_REF")]
		r#ref: Option<String>,
	},
}

#[tokio::main]
async fn main() -> Result<()> {
	let cli = Cli::parse();

	// setup panic hook
	utils::set_hook();
	utils::logs(cli.debug);

	let client = Client::new(cli.url, cli.token);

	if let Err(error) = handle_command(cli.command, &client).await {
		log::error!("{error}");
		log::debug!("{error:#?}");
		std::process::exit(1);
	}

	utils::clean_term();

	Ok(())
}

async fn handle_command(commands: Commands, client: &Client) -> Result<()> {
	match commands {
		Commands::Deploy { slug, r#ref } => {
			run_deploy(slug, r#ref.filter(|s| !s.is_empty()), client).await
		},
	}
}

async fn run_deploy(slug: String, r#ref: Option<String>, client: &Client) -> Result<()> {
	let stream = client.deploy(&slug, r#ref.as_deref());
	pin_mut!(stream);

	while let Some(event) = stream.next().await {
		match event? {
			Ok(Progress::Log(log)) => match log {
				Log::Info(message) => println!("{message}"),
				Log::Error(message) => eprintln!("{message}"),
			},
			Ok(Progress::Stage(stage)) => match stage {
				Stage::Deployed => log::info!("Deployed site"),
				Stage::Migrated => log::info!("Migrated database"),
				Stage::Starting => log::info!("Starting deployment"),
				Stage::Optimized => log::info!("Optimized deployment"),
				Stage::Downloaded => log::info!("Downloaded repository"),
				Stage::DepsInstalled => log::info!("Installed dependencies"),
			},
			Err(error) => return Err(error.into()),
		}
	}

	Ok(())
}
