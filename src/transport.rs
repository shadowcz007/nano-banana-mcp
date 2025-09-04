use anyhow::Result;
use crate::server::OpenRouterServer;
use rmcp::{service::ServiceExt, transport::stdio};

// SSE 所需
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use tokio_util::sync::CancellationToken;
use axum::serve;
use tower_http::cors::{Any, CorsLayer};
use axum::http::HeaderName;

pub async fn run_stdio(handler: OpenRouterServer) -> Result<()> {
	tracing::info!("Starting MCP server with stdio transport");
	let service = handler.serve(stdio()).await.inspect_err(|e| {
		tracing::error!("serving error: {:?}", e);
	})?;
	tracing::info!("MCP server started with stdio transport");
	service.waiting().await?;
	Ok(())
}

pub async fn run_sse(handler: OpenRouterServer) -> Result<()> {
	let config = handler.config.clone();
	let port = config.http_port;
	let bind_address = format!("127.0.0.1:{}", port);

	println!();
	println!("🚀 OpenRouter MCP Server (Rust) SSE 模式已启动!");
	println!("🔗 MCP 端点: http://{}/mcp", bind_address);
	println!("⏹️  按 Ctrl+C 停止服务器");
	println!();

	let server_config = SseServerConfig {
		bind: bind_address.parse()?,
		sse_path: "/mcp".to_string(),
		post_path: "/message".to_string(),
		ct: CancellationToken::new(),
		sse_keep_alive: None,
	};

	let (sse_server, router) = SseServer::new(server_config);
	let cors = CorsLayer::new()
		.allow_origin(Any)
		.allow_methods(Any)
		.allow_headers(vec![
			HeaderName::from_static("content-type"),
			HeaderName::from_static("authorization"),
		])
		.allow_credentials(false);
	let router_with_cors = router.layer(cors);
	let listener = tokio::net::TcpListener::bind(sse_server.config.bind).await?;
	let ct = sse_server.config.ct.child_token();
	let http = serve(listener, router_with_cors).with_graceful_shutdown(async move {
		ct.cancelled().await;
		tracing::info!("sse server cancelled");
	});
	
	tokio::spawn(async move {
		if let Err(e) = http.await {
			tracing::error!(error = %e, "sse server shutdown with error");
		}
	});

	let cancel_token = sse_server.with_service(move || handler.clone());
	println!("🌐 CORS 已启用，支持跨域访问");
	tokio::signal::ctrl_c().await?;
	cancel_token.cancel();
	Ok(())
} 