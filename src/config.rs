use anyhow::{anyhow, Result};
use std::env;

#[derive(Debug, Clone)]
pub struct OpenRouterConfig {
    pub api_key: String,
    pub base_url: String,
    pub http_referer: String,
    pub x_title: String,
    pub http_port: u16,
    pub model: String,
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

        // 获取模型配置：优先命令行参数，然后环境变量，最后默认值
        let model = Self::get_model_from_args(&args)
            .or_else(|| env::var("MCP_MODEL").ok())
            .unwrap_or_else(|| "google/gemini-2.5-flash-image-preview:free".to_string());

        // 验证模型是否在支持的列表中
        let supported_models = vec![
            "google/gemini-2.5-flash-image-preview:free".to_string(),
            "google/gemini-2.5-flash-image-preview".to_string(),
        ];
        
        if !supported_models.contains(&model) {
            return Err(anyhow!("不支持的模型: {}。支持的模型: {}", 
                model, 
                supported_models.join(", ")));
        }

        Ok(Self {
            api_key,
            base_url,
            http_referer,
            x_title,
            http_port,
            model,
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

    /// 从命令行参数中获取模型
    fn get_model_from_args(args: &[String]) -> Option<String> {
        for (i, arg) in args.iter().enumerate() {
            if arg == "--model" && i + 1 < args.len() {
                return Some(args[i + 1].clone());
            }
            if arg.starts_with("--model=") {
                return Some(arg.trim_start_matches("--model=").to_string());
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