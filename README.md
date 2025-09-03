# Nano Banana MCP

![](./assets/adf4681e-dc7f-4b2d-9ce6-0a1efa4bb79a.png)

一个轻量级的 Model Context Protocol (MCP) 服务器，提供 OpenRouter API 访问 google/gemini-2.5-flash-image 模型。

## 功能特性

- 🖼️ **图像生成**: 使用 AI 模型生成图像
- ✏️ **图像编辑**: 支持多图像输入的分析和编辑
- 🔧 **模型管理**: 动态切换不同的 AI 模型
- 💾 **文件管理**: 灵活的图片保存目录配置
- 🌐 **多传输方式**: 支持 stdio 和 SSE 传输
- 📁 **多格式支持**: 支持 URL、base64、本地文件路径
- 🔑 **灵活配置**: 支持环境变量和命令行参数设置 API key

## 支持的图像格式

- **URL 链接**: `https://example.com/image.jpg`
- **Base64 数据**: `data:image/jpeg;base64,...`
- **本地文件**: `./images/photo.png`, `C:\path\to\image.jpg`

## 快速开始

### 环境要求

- Rust 1.70+ (仅开发时需要)
- OpenRouter API 密钥

### 安装和使用

#### 方式 1: 使用预编译版本 (推荐)

1. 下载最新版本的 `nano-banana-mcp.exe` (Windows) 或 `nano-banana-mcp` (Linux/macOS)

2. 设置 API Key（选择一种方式）

   **方式 1: 环境变量**
   ```bash
   export OPENROUTER_API_KEY="your_api_key_here"
   ```

   **方式 2: 命令行参数**
   ```bash
   # 使用等号形式
   ./nano-banana-mcp --api-key=your_api_key_here
   
   # 使用空格形式
   ./nano-banana-mcp --api-key your_api_key_here
   ```

3. 运行程序
   ```bash
   # 使用 stdio 传输（默认）
   ./nano-banana-mcp

   # 使用 SSE 传输
   ./nano-banana-mcp sse

   # 使用 SSE 传输 + 命令行 API key
   ./nano-banana-mcp sse --api-key=your_api_key_here

   # 查看帮助
   ./nano-banana-mcp --help
   ```

#### 方式 2: 从源码编译

1. 克隆仓库
   ```bash
   git clone https://github.com/example/nano-banana-mcp.git
   cd nano-banana-mcp
   ```

2. 设置 API Key（选择一种方式）

   **方式 1: 环境变量**
   ```bash
   export OPENROUTER_API_KEY="your_api_key_here"
   ```

   **方式 2: 命令行参数**
   ```bash
   # 使用等号形式
   cargo run -- --api-key=your_api_key_here
   
   # 使用空格形式
   cargo run -- --api-key your_api_key_here
   ```

3. 编译和运行
   ```bash
   # 使用 stdio 传输（默认）
   cargo run

   # 使用 SSE 传输
   cargo run -- sse

   # 使用 SSE 传输 + 命令行 API key
   cargo run -- sse --api-key=your_api_key_here

   # 查看帮助
   cargo run -- --help
   ```

## 配置

### API Key 设置

支持两种方式设置 OpenRouter API 密钥：

1. **环境变量** (推荐用于生产环境)
   ```bash
   OPENROUTER_API_KEY=your_api_key_here
   ```

2. **命令行参数** (适用于临时使用或脚本)
   ```bash
   # 等号形式
   --api-key=your_api_key_here
   
   # 空格形式
   --api-key your_api_key_here
   ```

**优先级**: 命令行参数 > 环境变量

### 环境变量

- `OPENROUTER_API_KEY`: OpenRouter API 密钥（必需，如果未通过命令行参数提供）
- `MCP_HTTP_PORT`: SSE 传输时的 HTTP 端口（默认: 6621）

### 默认设置

- 默认模型: `google/gemini-2.5-flash-image-preview:free`
- 默认图片保存目录: `./images/`

## 使用示例

### 预编译版本用法

```bash
# 使用环境变量
export OPENROUTER_API_KEY="sk-xxx..."
./nano-banana-mcp

# 使用命令行参数
./nano-banana-mcp --api-key="sk-xxx..."

# SSE 模式 + 命令行 API key
./nano-banana-mcp sse --api-key="sk-xxx..."
```

### 开发模式用法

```bash
# 使用环境变量
export OPENROUTER_API_KEY="sk-xxx..."
cargo run

# 使用命令行参数
cargo run -- --api-key="sk-xxx..."

# SSE 模式 + 命令行 API key
cargo run -- sse --api-key="sk-xxx..."
```

### 在脚本中使用

```bash
#!/bin/bash
# 使用预编译版本
./nano-banana-mcp sse --api-key="$OPENROUTER_API_KEY"

# 或使用开发模式
cargo run -- sse --api-key="$OPENROUTER_API_KEY"
```

## 可用工具

### `generate_image`
生成图像，支持可选的参考图像输入。

```json
{
  "prompt": "一只可爱的小猫",
  "images": ["reference.jpg"]
}
```

### `edit_image`
编辑或分析图像，支持多图像输入。

```json
{
  "instruction": "将这张图片变成黑白风格",
  "images": ["color_image.jpg"]
}
```

### `set_model`
设置或获取当前使用的模型。

```json
{
  "model": "google/gemini-2.5-flash-image-preview"
}
```

### `set_save_directory`
设置或获取图片保存目录。

```json
{
  "save_directory": "./my_images/"
}
```

## 传输方式

### stdio 传输
适用于命令行工具和本地集成。

### SSE 传输
适用于 Web 应用和远程访问，支持 CORS。

## 开发

### 构建
```bash
# 开发版本
cargo build

# 发布版本
cargo build --release
```

### 测试
```bash
cargo test
```

### 代码检查
```bash
cargo check
cargo clippy
```

## 故障排除

### 常见问题

1. **API Key 错误**
   - 确保 API key 格式正确（以 `sk-` 开头）
   - 检查是否通过环境变量或命令行参数正确设置

2. **权限问题**
   - 确保 OpenRouter API key 有效且有足够权限
   - 检查账户余额和 API 限制

3. **网络问题**
   - 检查网络连接
   - 确认防火墙设置

4. **可执行文件权限问题** (Linux/macOS)
   ```bash
   chmod +x nano-banana-mcp
   ```

## 许可证

MIT License

## 贡献

欢迎提交 Issue 和 Pull Request！
