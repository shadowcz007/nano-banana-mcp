mod config;
mod image_utils;
mod cli;
mod server;
mod tools;
mod transport;

use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
		.with_writer(std::io::stderr)
		.with_ansi(false)
		.init();

	let args = cli::parse_args();
	let save_directory = args.save_directory.map(|p| p.to_string_lossy().to_string());
	
	let handler = server::OpenRouterServer::new(save_directory)?;
	match args.transport {
		cli::TransportType::Stdio => transport::run_stdio(handler).await?,
		cli::TransportType::Sse => transport::run_sse(handler).await?,
	}
	Ok(())
}
