mod config;
mod image_utils;

use anyhow::Result;
use config::OpenRouterConfig;
use rmcp::{
	service::ServiceExt,
	tool,
	tool_handler,
	tool_router,
	handler::server::router::tool::ToolRouter,
	handler::server::wrapper::Parameters,
	model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
	transport::stdio,
	ServerHandler,
	ErrorData as McpError,
	schemars,
};
use std::env;
use serde::Deserialize;
use serde_json::json;

// 新增：导入SSE传输相关模块
use rmcp::transport::sse_server::{SseServer, SseServerConfig};
use tokio_util::sync::CancellationToken;
use axum::serve;
use tower_http::cors::{Any, CorsLayer};
use axum::http::HeaderName;

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct GenerateImageArgs {
	#[schemars(example = &"一只可爱的小猫穿着宇航服在月球上行走，科幻风格")]
	pub prompt: String,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct EditImageArgs {
	#[schemars(example = &"请将这张图片编辑成一张科幻风格的海报")]
	pub instruction: String,
	#[schemars(example = &"https://example.com/image.jpg")]
	#[schemars(example = &"C:\\Images\\photo.png")]
	#[schemars(example = &"data:image/jpeg;base64,/9j/4AAQ...")]
	pub images: Vec<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetModelArgs {
	#[schemars(description = "要设置的模型名称，支持: google/gemini-2.5-flash-image-preview:free, google/gemini-2.5-flash-image-preview。如果为空或未提供，则返回当前设置的模型")]
	#[schemars(example = &"google/gemini-2.5-flash-image-preview:free")]
	#[serde(default)]
	pub model: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct SetSaveDirectoryArgs {
	#[schemars(description = "要设置的图片保存目录路径（必须是绝对路径）。如果为空或未提供，则返回当前设置的保存目录")]
	#[schemars(example = &"C:\\Users\\YourName\\Pictures")]
	#[schemars(example = &"/home/username/pictures")]
	#[serde(default)]
	pub save_directory: Option<String>,
}

#[derive(Clone)]
struct OpenRouterServer {
	tool_router: ToolRouter<Self>,
	config: OpenRouterConfig,
	client: reqwest::Client,
	current_model: std::sync::Arc<tokio::sync::RwLock<String>>,
	save_directory: std::sync::Arc<tokio::sync::RwLock<String>>,
}

#[tool_router]
impl OpenRouterServer {
	fn new(save_directory: Option<String>) -> Result<Self> {
		let config = OpenRouterConfig::from_env()?;
		let client = reqwest::Client::builder()
			.default_headers(config.get_headers())
			.build()?;
		
		// 确定保存目录：优先使用命令行参数，然后是环境变量，最后是默认值
		let save_dir = if let Some(cmd_save_dir) = save_directory {
			// 验证命令行提供的路径
			let path = std::path::Path::new(&cmd_save_dir);
			if !path.is_absolute() {
				return Err(anyhow::anyhow!("命令行参数 --save-directory 必须是绝对路径，当前提供: {}", cmd_save_dir));
			}
			cmd_save_dir
		} else if let Ok(env_save_dir) = std::env::var("MCP_SAVE_DIRECTORY") {
			// 检查环境变量
			let path = std::path::Path::new(&env_save_dir);
			if !path.is_absolute() {
				return Err(anyhow::anyhow!("环境变量 MCP_SAVE_DIRECTORY 必须是绝对路径，当前设置: {}", env_save_dir));
			}
			env_save_dir
		} else {
			// 获取当前工作目录并创建默认的 images 文件夹
			let current_dir = std::env::current_dir()?;
			let default_save_dir = current_dir.join("images");
			
			// 如果目录不存在，创建它
			if !default_save_dir.exists() {
				std::fs::create_dir_all(&default_save_dir)?;
			}
			
			default_save_dir.to_string_lossy().to_string()
		};
		
		// 确保保存目录存在且可写
		let path = std::path::Path::new(&save_dir);
		if !path.exists() {
			std::fs::create_dir_all(path)?;
		}
		
		if !path.is_dir() {
			return Err(anyhow::anyhow!("保存目录路径 '{}' 不是一个有效的目录", save_dir));
		}
		
		Ok(Self {
			tool_router: Self::tool_router(),
			config,
			client,
			current_model: std::sync::Arc::new(tokio::sync::RwLock::new("google/gemini-2.5-flash-image-preview:free".to_string())),
			save_directory: std::sync::Arc::new(tokio::sync::RwLock::new(save_dir)),
		})
	}

 
	#[tool(description = "文本生成图像")]
	async fn generate_image(&self, Parameters(args): Parameters<GenerateImageArgs>) -> Result<CallToolResult, McpError> {
		let url = format!("{}/chat/completions", self.config.base_url);
		// 使用当前设置的模型
		let model = {
			let current = self.current_model.read().await;
			current.clone()
		};
		
		// 构建消息内容
		let content = vec![json!({
			"type": "text",
			"text": args.prompt
		})];
		
		// generate_image 不需要处理图像输入，只需要文本提示词
		
		let request_body = json!({
			"model": model,
			"messages": [{
				"role": "user",
				"content": content
			}],
			"max_tokens": 1000,
			"temperature": 0.7
		});

		match self.client.post(&url).json(&request_body).send().await {
			Ok(response) => {
				// 首先检查 HTTP 状态码
				let status = response.status();
				if !status.is_success() {
					let error_text = response.text().await.unwrap_or_else(|_| "无法获取错误详情".to_string());
					return Err(McpError::internal_error(
						format!("API 请求失败，状态码: {}, 错误: {}", status, error_text), 
						None
					));
				}

				match response.json::<serde_json::Value>().await {
					Ok(response_data) => {
						// 添加调试信息，打印完整的响应
						eprintln!("API 响应: {}", serde_json::to_string_pretty(&response_data).unwrap_or_else(|_| "无法序列化响应".to_string()));
						
						// 检查是否有错误字段
						if let Some(error) = response_data.get("error") {
							let error_message = error.get("message")
								.and_then(|m| m.as_str())
								.unwrap_or("未知错误");
							return Err(McpError::internal_error(
								format!("API 返回错误: {}", error_message), 
								None
							));
						}

						// 检查 choices 字段
						let choices = response_data.get("choices")
							.and_then(|c| c.as_array())
							.ok_or_else(|| McpError::internal_error(
								"API 响应中缺少 'choices' 字段或格式不正确".to_string(), 
								None
							))?;

						if choices.is_empty() {
							return Err(McpError::internal_error(
								"API 响应中 'choices' 数组为空".to_string(), 
								None
							));
						}

						let choice = &choices[0];
						let message = choice.get("message")
							.ok_or_else(|| McpError::internal_error("消息格式无效".to_string(), None))?;
						
						let content = message.get("content")
							.and_then(|c| c.as_str())
							.unwrap_or("无内容");
						
						let empty_vec: Vec<serde_json::Value> = Vec::new();
						let images_array = message.get("images").and_then(|i| i.as_array()).unwrap_or(&empty_vec);
						
												// 使用当前设置的保存目录
						let current_save_dir = {
							let save_dir = self.save_directory.read().await;
							save_dir.clone()
						};
		let saved_images = image_utils::save_response_images(
			images_array, 
			Some(&current_save_dir),
			Some("generated_image"),
			false // 不是编辑模式
		);
		
		let mut response_text = format!("**模型:** {}\n**提示词:** {}\n**响应:** {}", 
			model, args.prompt, content);
						
						if !images_array.is_empty() {
							response_text.push_str(&format!("\n\n**生成的图像:** {} 张图像", images_array.len()));
							for (index, img_info) in saved_images.iter().enumerate() {
								response_text.push_str(&format!("\n- 图像 {}: {}...", index + 1, 
									&img_info.url[..std::cmp::min(50, img_info.url.len())]));
								if let Some(saved_path) = &img_info.saved_path {
									response_text.push_str(&format!("\n  已保存到: {}", saved_path));
								}
							}
						}

						// 添加使用统计
						if let Some(usage) = response_data.get("usage") {
							if let (Some(prompt_tokens), Some(completion_tokens), Some(total_tokens)) = (
								usage.get("prompt_tokens").and_then(|t| t.as_u64()),
								usage.get("completion_tokens").and_then(|t| t.as_u64()),
								usage.get("total_tokens").and_then(|t| t.as_u64())
							) {
								response_text.push_str(&format!("\n\n**使用统计:**\n- 提示词tokens: {}\n- 完成tokens: {}\n- 总tokens: {}", 
									prompt_tokens, completion_tokens, total_tokens));
							}
						}

						Ok(CallToolResult::success(vec![Content::text(response_text)]))
					}
					Err(e) => Err(McpError::internal_error(format!("解析响应失败: {}", e), None)),
				}
			}
			Err(e) => Err(McpError::internal_error(format!("请求失败: {}", e), None)),
		}
	}

	#[tool(description = "使用图像模型编辑或分析图像（支持多张图像）。图像可以是：1) URL链接 2) base64编码数据 3) 本地文件路径")]
	async fn edit_image(&self, Parameters(args): Parameters<EditImageArgs>) -> Result<CallToolResult, McpError> {
		// 验证是否传入了图片
		if args.images.is_empty() {
			return Err(McpError::internal_error(
				"❌ 编辑图像时必须传入至少一张图片！\n\n请提供以下格式之一的图片：\n- URL链接 (http:// 或 https://)\n- base64编码数据 (data:image/...)\n- 本地文件路径\n\n示例：\n- URL: https://example.com/image.jpg\n- 本地文件: C:\\Images\\photo.png\n- base64: data:image/jpeg;base64,/9j/4AAQ...", 
				None
			));
		}

		let url = format!("{}/chat/completions", self.config.base_url);
		// 使用当前设置的模型
		let model = {
			let current = self.current_model.read().await;
			current.clone()
		};
		
		// 构建包含文本指令和图像的内容数组
		let mut content = vec![json!({
			"type": "text",
			"text": args.instruction
		})];
		
		// 处理每个图像输入，支持多种格式
		for image_input in &args.images {
			// 首先尝试直接处理图像输入
			match image_utils::detect_and_process_image_input(image_input) {
				Ok(image_content) => {
					match image_content.content_type.as_str() {
						"url" => {
							// URL 格式，直接使用
							content.push(json!({
								"type": "image_url",
								"image_url": {
									"url": image_content.data
								}
							}));
						}
						"base64" => {
							// base64 格式，直接使用
							content.push(json!({
								"type": "image_url",
								"image_url": {
									"url": image_content.data
								}
							}));
						}
						_ => {
							// 其他格式，作为 base64 处理
							content.push(json!({
								"type": "image_url",
								"image_url": {
									"url": image_content.data
								}
							}));
						}
					}
				}
				Err(_) => {
					// 如果直接处理失败，尝试在 save_directory 中查找
					let current_save_dir = {
						let save_dir = self.save_directory.read().await;
						save_dir.clone()
					};
					
					match image_utils::find_image_in_save_directory(image_input, &current_save_dir) {
						Ok(image_content) => {
							// 在 save_directory 中找到图片，转换为 base64 格式
							content.push(json!({
								"type": "image_url",
								"image_url": {
									"url": image_content.data
								}
							}));
						}
						Err(e) => {
							// 在 save_directory 中也找不到，记录错误但继续处理其他图像
							eprintln!("处理图像输入 '{}' 失败: {}", image_input, e);
							// 尝试作为 URL 处理（保持向后兼容性）
							content.push(json!({
								"type": "image_url",
								"image_url": {
									"url": image_input
								}
							}));
						}
					}
				}
			}
		}

		let request_body = json!({
			"model": model,
			"messages": [{
				"role": "user",
				"content": content
			}],
			"max_tokens": 1000,
			"temperature": 0.7
		});

		match self.client.post(&url).json(&request_body).send().await {
			Ok(response) => {
				// 首先检查 HTTP 状态码
				let status = response.status();
				if !status.is_success() {
					let error_text = response.text().await.unwrap_or_else(|_| "无法获取错误详情".to_string());
					return Err(McpError::internal_error(
						format!("API 请求失败，状态码: {}, 错误: {}", status, error_text), 
						None
					));
				}

				match response.json::<serde_json::Value>().await {
					Ok(response_data) => {
						// 添加调试信息，打印完整的响应
						eprintln!("API 响应: {}", serde_json::to_string_pretty(&response_data).unwrap_or_else(|_| "无法序列化响应".to_string()));
						
						// 检查是否有错误字段
						if let Some(error) = response_data.get("error") {
							let error_message = error.get("message")
								.and_then(|m| m.as_str())
								.unwrap_or("未知错误");
							return Err(McpError::internal_error(
								format!("API 返回错误: {}", error_message), 
								None
							));
						}

						// 检查 choices 字段
						let choices = response_data.get("choices")
							.and_then(|c| c.as_array())
							.ok_or_else(|| McpError::internal_error(
								"API 响应中缺少 'choices' 字段或格式不正确".to_string(), 
								None
							))?;

						if choices.is_empty() {
							return Err(McpError::internal_error(
								"API 响应中 'choices' 数组为空".to_string(), 
								None
							));
						}

						let choice = &choices[0];
						let message = choice.get("message")
							.ok_or_else(|| McpError::internal_error("消息格式无效".to_string(), None))?;
						
						let content = message.get("content")
							.and_then(|c| c.as_str())
							.unwrap_or("无内容");
						
						let empty_vec: Vec<serde_json::Value> = Vec::new();
						let images_array = message.get("images").and_then(|i| i.as_array()).unwrap_or(&empty_vec);
						
						// 使用当前设置的保存目录
						let current_save_dir = {
							let save_dir = self.save_directory.read().await;
							save_dir.clone()
						};
						// 为编辑图像提取基础文件名（如果是本地图片）
						let base_filename = if !args.images.is_empty() {
							// 尝试从第一个本地图片路径提取文件名
							let first_image = &args.images[0];
							if !first_image.starts_with("http://") && !first_image.starts_with("https://") && !first_image.starts_with("data:image/") {
								// 这是一个本地文件路径，提取文件名
								Some(image_utils::extract_filename_without_extension(first_image))
							} else {
								None
							}
						} else {
							None
						};
						
						// 为编辑图像使用递增文件名，保留原文件名并添加 "edited" 标记
						let saved_images = image_utils::save_response_images(
							images_array, 
							Some(&current_save_dir),
							base_filename.as_deref(),
							true // 是编辑模式
						);
						
						let mut response_text = format!("**模型:** {}\n**指令:** {}\n**输入图像:** {} 张图像\n**响应:** {}", 
							model, args.instruction, args.images.len(), content);
						
						if !images_array.is_empty() {
							response_text.push_str(&format!("\n\n**生成的图像:** {} 张图像", images_array.len()));
							for (index, img_info) in saved_images.iter().enumerate() {
								response_text.push_str(&format!("\n- 图像 {}: {}...", index + 1, 
									&img_info.url[..std::cmp::min(50, img_info.url.len())]));
								if let Some(saved_path) = &img_info.saved_path {
									response_text.push_str(&format!("\n  已保存到: {}", saved_path));
								}
							}
						}

						// 添加使用统计
						if let Some(usage) = response_data.get("usage") {
							if let (Some(prompt_tokens), Some(completion_tokens), Some(total_tokens)) = (
								usage.get("prompt_tokens").and_then(|t| t.as_u64()),
								usage.get("completion_tokens").and_then(|t| t.as_u64()),
								usage.get("total_tokens").and_then(|t| t.as_u64())
							) {
								response_text.push_str(&format!("\n\n**使用统计:**\n- 提示词tokens: {}\n- 完成tokens: {}\n- 总tokens: {}", 
									prompt_tokens, completion_tokens, total_tokens));
							}
						}

						Ok(CallToolResult::success(vec![Content::text(response_text)]))
					}
					Err(e) => Err(McpError::internal_error(format!("解析响应失败: {}", e), None)),
				}
			}
			Err(e) => Err(McpError::internal_error(format!("请求失败: {}", e), None)),
		}
	}

	#[tool(description = "设置或获取当前使用的模型")]
	async fn set_model(&self, Parameters(args): Parameters<SetModelArgs>) -> Result<CallToolResult, McpError> {
		match args.model {
			Some(new_model) => {
				// 验证模型名称是否在支持的列表中
				let supported_models = vec![
					"google/gemini-2.5-flash-image-preview:free".to_string(),
					"google/gemini-2.5-flash-image-preview".to_string(),
				];
				
				if !supported_models.contains(&new_model) {
					return Err(McpError::internal_error(
						format!("不支持的模型: {}。支持的模型: {}", 
							new_model, 
							supported_models.join(", ")), 
						None
					));
				}
				
				// 设置新模型
				{
					let mut current = self.current_model.write().await;
					*current = new_model.clone();
				}
				
				Ok(CallToolResult::success(vec![Content::text(format!(
					"✅ 模型已成功设置为: **{}**", 
					new_model
				))]))
			}
			None => {
				// 返回当前设置的模型
				let current = self.current_model.read().await;
				Ok(CallToolResult::success(vec![Content::text(format!(
					"📋 当前设置的模型: **{}**", 
					*current
				))]))
			}
		}
	}

	#[tool(description = "设置或获取图片保存目录。注意：只接受绝对路径，不支持相对路径")]
	async fn set_save_directory(&self, Parameters(args): Parameters<SetSaveDirectoryArgs>) -> Result<CallToolResult, McpError> {
		match args.save_directory {
			Some(new_directory) => {
				// 验证目录路径是否有效
				let path = std::path::Path::new(&new_directory);
				
				// 检查是否为绝对路径
				if !path.is_absolute() {
					return Err(McpError::internal_error(
						format!("路径 '{}' 是相对路径。请提供绝对路径，例如：\n- Windows: C:\\Users\\YourName\\Pictures\n- Linux/Mac: /home/username/pictures", new_directory), 
						None
					));
				}
				
				// 如果目录不存在，尝试创建它
				if !path.exists() {
					match std::fs::create_dir_all(path) {
						Ok(_) => {},
						Err(e) => {
							return Err(McpError::internal_error(
								format!("无法创建目录 '{}': {}", new_directory, e), 
								None
							));
						}
					}
				}
				
				// 验证目录是否可写
				if !path.is_dir() {
					return Err(McpError::internal_error(
						format!("'{}' 不是一个有效的目录", new_directory), 
						None
					));
				}
				
				// 设置新保存目录
				{
					let mut current = self.save_directory.write().await;
					*current = new_directory.clone();
				}
				
				Ok(CallToolResult::success(vec![Content::text(format!(
					"✅ 图片保存目录已成功设置为: **{}**", 
					new_directory
				))]))
			}
			None => {
				// 返回当前设置的保存目录
				let current = self.save_directory.read().await;
				Ok(CallToolResult::success(vec![Content::text(format!(
					"📁 当前设置的图片保存目录: **{}**", 
					*current
				))]))
			}
		}
	}
}

#[tool_handler]
impl ServerHandler for OpenRouterServer {
	fn get_info(&self) -> ServerInfo {
		ServerInfo {
			instructions: Some("nano banana MCP - 提供 OpenRouter API 访问 google/gemini-2.5-flash-image模型。支持多种图像输入格式：URL、base64、本地文件路径。可用工具: generate_image, edit_image, set_model, set_save_directory".into()),
			capabilities: ServerCapabilities::builder()
				.enable_tools()
				.enable_resources()
				.build(),
			..Default::default()
		}
	}
}

fn print_usage() {
	// 检测是否为 release 模式
	let is_release = cfg!(debug_assertions) == false;
	let program_name = if is_release { "nano-banana-mcp" } else { "cargo run" };
	
	println!("nano banana MCP Server");
	println!("用法:");
	if is_release {
		println!("  {}                                    # 启动 MCP 服务器 (默认使用 stdio)", program_name);
		println!("  {} sse                                 # 使用 SSE 传输", program_name);
		println!("  {} --help                              # 显示此帮助信息", program_name);
	} else {
		println!("  {}                                    # 启动 MCP 服务器 (默认使用 stdio)", program_name);
		println!("  {} -- sse                             # 使用 SSE 传输", program_name);
		println!("  {} -- --help                          # 显示此帮助信息", program_name);
	}
	println!();
	println!("命令行参数:");
	println!("  --api-key=KEY                             # 设置 OpenRouter API 密钥");
	println!("  --save-directory=PATH                     # 设置图片保存目录 (必须是绝对路径)");
	println!("  -s PATH                                   # --save-directory 的简写形式");
	println!();
	println!("API Key 设置 (选择一种方式):");
	println!("  1. 环境变量: OPENROUTER_API_KEY=your_key");
	println!("  2. 命令行参数: --api-key=your_key 或 --api-key your_key");
	println!();
	println!("环境变量:");
	println!("  OPENROUTER_API_KEY                           # OpenRouter API 密钥 (必须)");
	println!("  MCP_HTTP_PORT                                # SSE 传输时的 HTTP 端口 (默认: 6621)");
	println!("  MCP_SAVE_DIRECTORY                           # 图片保存目录 (必须是绝对路径)");
	println!();
	println!("示例:");
	if is_release {
		println!("  {} --api-key=sk-xxx...                   # 使用命令行参数设置 API key", program_name);
		println!("  {} --save-directory=C:\\Images            # 设置图片保存目录", program_name);
		println!("  {} --api-key=sk-xxx... --save-directory=C:\\Images  # 同时设置两个参数", program_name);
		println!("  {} sse --api-key=sk-xxx...               # SSE 模式 + 命令行 API key", program_name);
		println!("  {} sse --save-directory=/home/user/images # SSE 模式 + 保存目录", program_name);
		println!("  OPENROUTER_API_KEY=sk-xxx... {}          # 使用环境变量", program_name);
		println!("  MCP_SAVE_DIRECTORY=C:\\Images {}          # 使用环境变量设置保存目录", program_name);
	} else {
		println!("  {} -- --api-key=sk-xxx...                # 使用命令行参数设置 API key", program_name);
		println!("  {} -- --save-directory=C:\\Images         # 设置图片保存目录", program_name);
		println!("  {} -- --api-key=sk-xxx... --save-directory=C:\\Images  # 同时设置两个参数", program_name);
		println!("  {} -- sse --api-key=sk-xxx...            # SSE 模式 + 命令行 API key", program_name);
		println!("  {} -- sse --save-directory=/home/user/images # SSE 模式 + 保存目录", program_name);
		println!("  OPENROUTER_API_KEY=sk-xxx... {}          # 使用环境变量", program_name);
		println!("  MCP_SAVE_DIRECTORY=C:\\Images {}          # 使用环境变量设置保存目录", program_name);
	}
}

#[tokio::main]
async fn main() -> Result<()> {
	// 改进tracing配置，参考示例代码
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::DEBUG.into()))
		.with_writer(std::io::stderr)
		.with_ansi(false)
		.init();

	let args: Vec<String> = env::args().collect();
	
	if args.contains(&"--help".to_string()) {
		print_usage();
		return Ok(());
	}

	// 先解析传输方式，过滤掉 --api-key 和 --save-directory 相关参数
	let mut transport_type = "stdio"; // 默认值
	let mut save_directory: Option<String> = None;
	let mut i = 1;
	
	while i < args.len() {
		let arg = &args[i];
		if arg == "stdio" || arg == "sse" {
			transport_type = arg;
			break;
		} else if arg.starts_with("--api-key") || arg == "--api-key" {
			// 跳过 --api-key 参数
			if arg == "--api-key" && i + 1 < args.len() {
				i += 2; // 跳过 --api-key 和它的值
			} else {
				i += 1; // 跳过 --api-key=value
			}
		} else if arg == "--save-directory" || arg == "-s" {
			// 处理 --save-directory 参数
			if i + 1 < args.len() {
				save_directory = Some(args[i + 1].clone());
				i += 2; // 跳过 --save-directory 和它的值
			} else {
				eprintln!("错误: 缺少 --save-directory 的值");
				println!();
				print_usage();
				std::process::exit(1);
			}
		} else {
			i += 1;
		}
	}

	let handler = OpenRouterServer::new(save_directory)?;

	// 使用解析出的传输方式
	match transport_type {
		"stdio" => {
			// 使用 stdio 传输，参考示例代码改进错误处理
			tracing::info!("Starting MCP server with stdio transport");
			
			let service = handler.serve(stdio()).await.inspect_err(|e| {
				tracing::error!("serving error: {:?}", e);
			})?;
			
			tracing::info!("MCP server started with stdio transport");
			service.waiting().await?;
		}
		"sse" => {
			// 使用 SSE 传输 - 参考ScreenTime的实现方式
			let config = handler.config.clone();
			let port = config.http_port;
			let bind_address = format!("127.0.0.1:{}", port);

			println!();
			println!("🚀 OpenRouter MCP Server (Rust) SSE 模式已启动!");
			println!("🔗 MCP 端点: http://{}/mcp", bind_address); 
			println!("⏹️  按 Ctrl+C 停止服务器");
			println!();

			// 使用rmcp的SSE传输配置
			let server_config = SseServerConfig {
				bind: bind_address.parse()?,
				sse_path: "/mcp".to_string(),
				post_path: "/message".to_string(),
				ct: CancellationToken::new(),
				sse_keep_alive: None,
			};

			let (sse_server, router) = SseServer::new(server_config);
			
			// 添加 CORS 中间件
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
		}
		_ => {
			eprintln!("错误: 不支持的传输方式 '{}'", transport_type);
			println!();
			print_usage();
			std::process::exit(1);
		}
	}
	
	Ok(())
}
