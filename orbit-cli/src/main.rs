use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use orbit_client::Client;
use orbit_core::{Log, Progress, Stage};
use url::Url;

#[derive(Debug, Parser)]
struct Cli {
	/// URL to the Orbit instance.
	#[arg(short, long, env = "ORBIT_URL")]
	url: Url,

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
async fn main() {
	let cli = Cli::parse();
	let client = Client::new(cli.url);

	match cli.command {
		Commands::Deploy { slug, r#ref } => run_deploy(slug, r#ref, &client).await,
	}
}

async fn run_deploy(slug: String, r#ref: Option<String>, client: &Client) {
	let stream = client.deploy(&slug, r#ref.as_deref()).await;

	stream
		.map(|result| result.unwrap())
		.for_each(|event| async {
			match event {
				Ok(Progress::Log(log)) => match log {
					Log::Info(message) => println!("{message}"),
					Log::Error(message) => eprintln!("{message}"),
				},
				Ok(Progress::Stage(stage)) => match stage {
					Stage::Starting => println!("Starting deployment"),
					Stage::Downloaded => println!("Downloaded repository"),
					Stage::DepsInstalled => println!("Installed dependencies"),
					Stage::Deployed => println!("Deployed site"),
				},
				Err(error) => eprintln!("{error}"),
			}
		})
		.await;
}
