use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::env;

#[derive(Debug, Clone)]
pub struct OpenRouterConfig {
    pub api_key: String,
    pub base_url: String,
    pub http_referer: String,
    pub x_title: String,
    pub http_port: u16,
}

impl OpenRouterConfig {
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok(); // 加载 .env 文件，如果存在

        // 首先尝试从命令行参数获取 API key
        let args: Vec<String> = env::args().collect();
        let api_key = Self::get_api_key_from_args(&args)
            .or_else(|| env::var("OPENROUTER_API_KEY").ok())
            .ok_or_else(|| anyhow!("OPENROUTER_API_KEY 环境变量或 --api-key 命令行参数是必需的"))?;

        let base_url = env::var("OPENROUTER_BASE_URL")
            .unwrap_or_else(|_| "https://openrouter.ai/api/v1".to_string());

        let http_referer = env::var("HTTP_REFERER")
            .unwrap_or_else(|_| "http://localhost:3000".to_string());

        let x_title = env::var("X_TITLE")
            .unwrap_or_else(|_| "OpenRouter MCP Server (Rust)".to_string());

        let http_port = env::var("MCP_HTTP_PORT")
            .unwrap_or_else(|_| "6621".to_string())
            .parse()
            .unwrap_or(6621);

        Ok(Self {
            api_key,
            base_url,
            http_referer,
            x_title,
            http_port,
        })
    }

    /// 从命令行参数中获取 API key
    fn get_api_key_from_args(args: &[String]) -> Option<String> {
        for (i, arg) in args.iter().enumerate() {
            if arg == "--api-key" && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
            if arg.starts_with("--api-key=") {
                return Some(arg.trim_start_matches("--api-key=").to_string());
            }
        }
        None
    }

    pub fn get_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        
        headers.insert(
            reqwest::header::AUTHORIZATION,
            format!("Bearer {}", self.api_key).parse().unwrap(),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("http-referer"),
            self.http_referer.parse().unwrap(),
        );
        headers.insert(
            reqwest::header::HeaderName::from_static("x-title"),
            self.x_title.parse().unwrap(),
        );
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            "application/json".parse().unwrap(),
        );

        headers
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OpenRouterModel {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub context_length: Option<u32>,
    pub pricing: Option<ModelPricing>,
    pub top_provider: Option<ModelProvider>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ModelPricing {
    pub prompt: String,
    pub completion: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ModelProvider {
    pub max_completion_tokens: Option<u32>,
    pub is_moderated: Option<bool>,
}

#[derive(Debug, Deserialize)]
pub struct ModelsResponse {
    pub data: Vec<OpenRouterModel>,
}

// 聊天相关结构体
#[derive(Debug, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: serde_json::Value, // 可以是字符串或数组（多模态）
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct ChatResponse {
    pub choices: Vec<ChatChoice>,
    pub usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
    pub finish_reason: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChatUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// 新增：图像相关结构体
#[derive(Debug, Deserialize)]
pub struct ImageResponse {
    pub url: String,
    pub detail: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ChatMessageWithImages {
    pub content: String,
    pub images: Option<Vec<ImageResponse>>,
}

// 新增：工具请求参数结构体
#[derive(Debug, Deserialize)]
pub struct ChatWithModelParams {
    pub model: String,
    pub message: serde_json::Value, // 可以是字符串或数组
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub system_prompt: Option<String>,
    pub save_directory: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CompareModelsParams {
    pub models: Vec<String>,
    pub message: serde_json::Value,
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct GenerateImageParams {
    pub model: Option<String>,
    pub prompt: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub save_directory: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct EditImageParams {
    pub model: Option<String>,
    pub instruction: String,
    pub images: Vec<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub save_directory: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GetModelInfoParams {
    pub model: String,
} 