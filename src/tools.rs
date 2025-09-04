use anyhow::Result;
use crate::{image_utils, server::OpenRouterServer};
use rmcp::{
	tool,
	tool_router,
	handler::server::wrapper::Parameters,
	model::{CallToolResult, Content},
	ErrorData as McpError,
	schemars,
};
use serde::Deserialize;
use serde_json::json;

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

#[tool_router]
impl OpenRouterServer {
	#[tool(description = "文本生成图像")]
	async fn generate_image(&self, Parameters(args): Parameters<GenerateImageArgs>) -> Result<CallToolResult, McpError> {
		let url = format!("{}/chat/completions", self.config.base_url);
		let model = self.config.model.clone();
		let content = vec![json!({
			"type": "text",
			"text": args.prompt
		})];
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
						eprintln!("API 响应: {}", serde_json::to_string_pretty(&response_data).unwrap_or_else(|_| "无法序列化响应".to_string()));
						if let Some(error) = response_data.get("error") {
							let error_message = error.get("message").and_then(|m| m.as_str()).unwrap_or("未知错误");
							return Err(McpError::internal_error(format!("API 返回错误: {}", error_message), None));
						}

						let choices = response_data.get("choices").and_then(|c| c.as_array()).ok_or_else(|| McpError::internal_error(
							"API 响应中缺少 'choices' 字段或格式不正确".to_string(), None
						))?;
						if choices.is_empty() {
							return Err(McpError::internal_error("API 响应中 'choices' 数组为空".to_string(), None));
						}

						let choice = &choices[0];
						let message = choice.get("message").ok_or_else(|| McpError::internal_error("消息格式无效".to_string(), None))?;
						let content = message.get("content").and_then(|c| c.as_str()).unwrap_or("无内容");
						let empty_vec: Vec<serde_json::Value> = Vec::new();
						let images_array = message.get("images").and_then(|i| i.as_array()).unwrap_or(&empty_vec);

						let current_save_dir = {
							let save_dir = self.save_directory.read().await;
							save_dir.clone()
						};
						let saved_images = image_utils::save_response_images(
							images_array, 
							Some(&current_save_dir),
							Some("generated_image"),
							false
						);

						let mut response_text = format!("**模型:** {}\n**提示词:** {}\n**响应:** {}", model, args.prompt, content);
						if !images_array.is_empty() {
							response_text.push_str(&format!("\n\n**生成的图像:** {} 张图像", images_array.len()));
							for (index, img_info) in saved_images.iter().enumerate() {
								response_text.push_str(&format!("\n- 图像 {}: {}...", index + 1, &img_info.url[..std::cmp::min(50, img_info.url.len())]));
								if let Some(saved_path) = &img_info.saved_path { response_text.push_str(&format!("\n  已保存到: {}", saved_path)); }
							}
						}

						if let Some(usage) = response_data.get("usage") {
							if let (Some(prompt_tokens), Some(completion_tokens), Some(total_tokens)) = (
								usage.get("prompt_tokens").and_then(|t| t.as_u64()),
								usage.get("completion_tokens").and_then(|t| t.as_u64()),
								usage.get("total_tokens").and_then(|t| t.as_u64())
							) {
								response_text.push_str(&format!("\n\n**使用统计:**\n- 提示词tokens: {}\n- 完成tokens: {}\n- 总tokens: {}", prompt_tokens, completion_tokens, total_tokens));
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
		if args.images.is_empty() {
			return Err(McpError::internal_error(
				"❌ 编辑图像时必须传入至少一张图片！\n\n请提供以下格式之一的图片：\n- URL链接 (http:// 或 https://)\n- base64编码数据 (data:image/...)\n- 本地文件路径\n\n示例：\n- URL: https://example.com/image.jpg\n- 本地文件: C:\\Images\\photo.png\n- base64: data:image/jpeg;base64,/9j/4AAQ...",
				None
			));
		}

		let url = format!("{}/chat/completions", self.config.base_url);
		let model = self.config.model.clone();
		let mut content = vec![json!({
			"type": "text",
			"text": args.instruction
		})];

		for image_input in &args.images {
			match image_utils::detect_and_process_image_input(image_input) {
				Ok(image_content) => {
					match image_content.content_type.as_str() {
						"url" => {
							content.push(json!({
								"type": "image_url",
								"image_url": {"url": image_content.data}
							}));
						}
						"base64" => {
							content.push(json!({
								"type": "image_url",
								"image_url": {"url": image_content.data}
							}));
						}
						_ => {
							content.push(json!({
								"type": "image_url",
								"image_url": {"url": image_content.data}
							}));
						}
					}
				}
				Err(_) => {
					let current_save_dir = {
						let save_dir = self.save_directory.read().await;
						save_dir.clone()
					};
					match image_utils::find_image_in_save_directory(image_input, &current_save_dir) {
						Ok(image_content) => {
							content.push(json!({
								"type": "image_url",
								"image_url": {"url": image_content.data}
							}));
						}
						Err(e) => {
							eprintln!("处理图像输入 '{}' 失败: {}", image_input, e);
							content.push(json!({
								"type": "image_url",
								"image_url": {"url": image_input}
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
						eprintln!("API 响应: {}", serde_json::to_string_pretty(&response_data).unwrap_or_else(|_| "无法序列化响应".to_string()));
						if let Some(error) = response_data.get("error") {
							let error_message = error.get("message").and_then(|m| m.as_str()).unwrap_or("未知错误");
							return Err(McpError::internal_error(format!("API 返回错误: {}", error_message), None));
						}

						let choices = response_data.get("choices").and_then(|c| c.as_array()).ok_or_else(|| McpError::internal_error(
							"API 响应中缺少 'choices' 字段或格式不正确".to_string(), None
						))?;
						if choices.is_empty() {
							return Err(McpError::internal_error("API 响应中 'choices' 数组为空".to_string(), None));
						}

						let choice = &choices[0];
						let message = choice.get("message").ok_or_else(|| McpError::internal_error("消息格式无效".to_string(), None))?;
						let content = message.get("content").and_then(|c| c.as_str()).unwrap_or("无内容");
						let empty_vec: Vec<serde_json::Value> = Vec::new();
						let images_array = message.get("images").and_then(|i| i.as_array()).unwrap_or(&empty_vec);

						let current_save_dir = {
							let save_dir = self.save_directory.read().await;
							save_dir.clone()
						};

						let base_filename = if !args.images.is_empty() {
							let first_image = &args.images[0];
							if !first_image.starts_with("http://") && !first_image.starts_with("https://") && !first_image.starts_with("data:image/") {
								Some(image_utils::extract_filename_without_extension(first_image))
							} else { None }
						} else { None };

						let saved_images = image_utils::save_response_images(
							images_array, 
							Some(&current_save_dir),
							base_filename.as_deref(),
							true
						);

						let mut response_text = format!("**模型:** {}\n**指令:** {}\n**输入图像:** {} 张图像\n**响应:** {}", model, args.instruction, args.images.len(), content);
						if !images_array.is_empty() {
							response_text.push_str(&format!("\n\n**生成的图像:** {} 张图像", images_array.len()));
							for (index, img_info) in saved_images.iter().enumerate() {
								response_text.push_str(&format!("\n- 图像 {}: {}...", index + 1, &img_info.url[..std::cmp::min(50, img_info.url.len())]));
								if let Some(saved_path) = &img_info.saved_path { response_text.push_str(&format!("\n  已保存到: {}", saved_path)); }
							}
						}

						if let Some(usage) = response_data.get("usage") {
							if let (Some(prompt_tokens), Some(completion_tokens), Some(total_tokens)) = (
								usage.get("prompt_tokens").and_then(|t| t.as_u64()),
								usage.get("completion_tokens").and_then(|t| t.as_u64()),
								usage.get("total_tokens").and_then(|t| t.as_u64())
							) {
								response_text.push_str(&format!("\n\n**使用统计:**\n- 提示词tokens: {}\n- 完成tokens: {}\n- 总tokens: {}", prompt_tokens, completion_tokens, total_tokens));
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
}

impl OpenRouterServer {
	pub(crate) fn create_tool_router() -> rmcp::handler::server::router::tool::ToolRouter<Self> {
		Self::tool_router()
	}
} 