use clap::{Parser, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum TransportType {
    #[value(name = "stdio")]
    Stdio,
    #[value(name = "sse")]
    Sse,
}

impl Default for TransportType {
    fn default() -> Self {
        Self::Stdio
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "nano-banana-mcp",
    about = "nano banana MCP Server - 提供 OpenRouter API 访问 google/gemini-2.5-flash-image 模型",
    long_about = "支持多种图像输入格式：URL、base64、本地文件路径。可用工具: generate_image, edit_image。"
)]
pub struct CliArgs {
    /// 传输类型：stdio 或 sse
    #[arg(value_enum, default_value_t = TransportType::Stdio)]
    pub transport: TransportType,

    /// 设置 OpenRouter API 密钥
    #[arg(long, env = "OPENROUTER_API_KEY", help = "设置 OpenRouter API 密钥")]
    pub api_key: Option<String>,

    /// 设置使用的模型
    #[arg(long, env = "MCP_MODEL", help = "设置使用的模型")]
    pub model: Option<String>,

    /// 设置图片保存目录 (必须是绝对路径)
    #[arg(short = 's', long, env = "MCP_SAVE_DIRECTORY", help = "设置图片保存目录 (必须是绝对路径)")]
    pub save_directory: Option<PathBuf>,
}

pub fn parse_args() -> CliArgs {
    CliArgs::parse()
} 