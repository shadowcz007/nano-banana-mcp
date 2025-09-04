use anyhow::Result;
use crate::config::OpenRouterConfig;
use rmcp::{
	handler::server::router::tool::ToolRouter,
	model::{ServerCapabilities, ServerInfo},
	ServerHandler,
	tool_handler,
};

#[derive(Clone)]
pub struct OpenRouterServer {
	pub(crate) tool_router: ToolRouter<Self>,
	pub(crate) config: OpenRouterConfig,
	pub(crate) client: reqwest::Client,
	pub(crate) save_directory: std::sync::Arc<tokio::sync::RwLock<String>>,
}

impl OpenRouterServer {
	pub fn new(save_directory: Option<String>) -> Result<Self> {
		let config = OpenRouterConfig::from_env()?;
		let client = reqwest::Client::builder()
			.default_headers(config.get_headers())
			.build()?;
		
		let save_dir = if let Some(cmd_save_dir) = save_directory {
			let path = std::path::Path::new(&cmd_save_dir);
			if !path.is_absolute() {
				return Err(anyhow::anyhow!("命令行参数 --save-directory 必须是绝对路径，当前提供: {}", cmd_save_dir));
			}
			cmd_save_dir
		} else if let Ok(env_save_dir) = std::env::var("MCP_SAVE_DIRECTORY") {
			let path = std::path::Path::new(&env_save_dir);
			if !path.is_absolute() {
				return Err(anyhow::anyhow!("环境变量 MCP_SAVE_DIRECTORY 必须是绝对路径，当前设置: {}", env_save_dir));
			}
			env_save_dir
		} else {
			let current_dir = std::env::current_dir()?;
			let default_save_dir = current_dir.join("images");
			if !default_save_dir.exists() {
				std::fs::create_dir_all(&default_save_dir)?;
			}
			default_save_dir.to_string_lossy().to_string()
		};
		
		let path = std::path::Path::new(&save_dir);
		if !path.exists() {
			std::fs::create_dir_all(path)?;
		}
		if !path.is_dir() {
			return Err(anyhow::anyhow!("保存目录路径 '{}' 不是一个有效的目录", save_dir));
		}
		
		Ok(Self {
			tool_router: Self::create_tool_router(),
			config,
			client,
			save_directory: std::sync::Arc::new(tokio::sync::RwLock::new(save_dir)),
		})
	}
}

#[tool_handler]
impl ServerHandler for OpenRouterServer {
	fn get_info(&self) -> ServerInfo {
		ServerInfo {
			instructions: Some("nano banana MCP - 提供 OpenRouter API 访问 google/gemini-2.5-flash-image模型。支持多种图像输入格式：URL、base64、本地文件路径。可用工具: generate_image, edit_image。模型和保存目录只能通过命令行参数或环境变量设置。".into()),
			capabilities: ServerCapabilities::builder()
				.enable_tools()
				.enable_resources()
				.build(),
			..Default::default()
		}
	}
} 