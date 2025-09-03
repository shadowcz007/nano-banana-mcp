use std::fs;
use std::path::Path;
use std::io::Write;
use base64::{Engine as _, engine::general_purpose};
use anyhow::{Result, anyhow};
use chrono::Utc;

/// 生成递增的文件名，避免重复
fn generate_incremental_filename(base_name: &str, extension: &str, directory: &str) -> String {
    let dir_path = Path::new(directory);
    let mut counter = 1;
    
    loop {
        let filename = if counter == 1 {
            format!("{}.{}", base_name, extension)
        } else {
            format!("{}_{}.{}", base_name, counter, extension)
        };
        
        let filepath = dir_path.join(&filename);
        if !filepath.exists() {
            return filename;
        }
        counter += 1;
    }
}

/// 从本地文件路径提取文件名（不含扩展名）
pub fn extract_filename_without_extension(file_path: &str) -> String {
    let path = Path::new(file_path);
    path.file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("image")
        .to_string()
}

/// 保存base64图像到文件系统
pub fn save_base64_image(base64_data: &str, directory: &str, filename: Option<&str>) -> Result<String> {
    // 确保目录存在
    let dir_path = Path::new(directory);
    if !dir_path.exists() {
        fs::create_dir_all(dir_path)?;
    }

    // 从base64数据中提取图像格式
    let captures = base64_data
        .split(";base64,")
        .collect::<Vec<&str>>();
    
    if captures.len() != 2 {
        return Err(anyhow!("无效的base64图像数据格式"));
    }

    let mime_part = captures[0];
    let actual_base64_data = captures[1];

    // 提取图像类型
    let image_type = mime_part
        .split("data:image/")
        .nth(1)
        .ok_or_else(|| anyhow!("无法解析图像类型"))?;

    // 生成文件名（如果未提供）
    let final_filename = if let Some(name) = filename {
        if !name.contains('.') {
            format!("{}.{}", name, image_type)
        } else {
            name.to_string()
        }
    } else {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        format!("image_{}.{}", timestamp, image_type)
    };

    let filepath = dir_path.join(&final_filename);

    // 将base64转换为字节并保存
    let image_bytes = general_purpose::STANDARD.decode(actual_base64_data)
        .map_err(|e| anyhow!("base64解码失败: {}", e))?;
    
    let mut file = fs::File::create(&filepath)?;
    file.write_all(&image_bytes)?;

    Ok(filepath.to_string_lossy().to_string())
}

/// 保存OpenRouter API的响应图像，支持递增文件名
pub fn save_response_images(
    images: &[serde_json::Value], 
    save_directory: Option<&str>,
    base_filename: Option<&str>,
    is_edit: bool
) -> Vec<ImageInfo> {
    if let Some(dir) = save_directory {
        if let Ok(_dir_path) = fs::canonicalize(dir) {
            return images.iter().enumerate().map(|(index, img)| {
                let image_url = img.get("image_url")
                    .and_then(|url_obj| url_obj.get("url"))
                    .and_then(|url| url.as_str())
                    .unwrap_or("");

                let mut image_info = ImageInfo {
                    url: image_url.to_string(),
                    saved_path: None,
                };

                if image_url.starts_with("data:image/") {
                    // 生成递增的文件名
                    let filename = if let Some(base_name) = base_filename {
                        if is_edit {
                            // 编辑模式：保留原文件名，添加 "edited" 标记
                            let base_with_edited = format!("{}_edited", base_name);
                            generate_incremental_filename(&base_with_edited, "png", dir)
                        } else {
                            // 生成模式：使用基础名称
                            generate_incremental_filename(base_name, "png", dir)
                        }
                    } else {
                        // 默认文件名
                        let default_name = if is_edit { "edited_image" } else { "generated_image" };
                        generate_incremental_filename(&format!("{}_{}", default_name, index + 1), "png", dir)
                    };

                    match save_base64_image(image_url, dir, Some(&filename)) {
                        Ok(saved_path) => {
                            image_info.saved_path = Some(saved_path);
                        }
                        Err(e) => {
                            eprintln!("保存图像 {} 失败: {}", index + 1, e);
                        }
                    }
                }

                image_info
            }).collect();
        }
    }

    // 如果没有指定保存目录，只返回URL信息
    images.iter().map(|img| {
        let image_url = img.get("image_url")
            .and_then(|url_obj| url_obj.get("url"))
            .and_then(|url| url.as_str())
            .unwrap_or("");
        
        ImageInfo {
            url: image_url.to_string(),
            saved_path: None,
        }
    }).collect()
}

#[derive(Debug)]
pub struct ImageInfo {
    pub url: String,
    pub saved_path: Option<String>,
} 

/// 检测图片输入类型并返回标准化的内容格式
pub fn detect_and_process_image_input(image_input: &str) -> Result<ImageContent> {
    // 检测是否为 base64 数据
    if image_input.starts_with("data:image/") {
        return Ok(ImageContent {
            content_type: "base64".to_string(),
            data: image_input.to_string(),
            mime_type: extract_mime_type_from_base64(image_input)?,
        });
    }
    
    // 检测是否为 URL
    if image_input.starts_with("http://") || image_input.starts_with("https://") {
        return Ok(ImageContent {
            content_type: "url".to_string(),
            data: image_input.to_string(),
            mime_type: "image/*".to_string(),
        });
    }
    
    // 检测是否为本地文件路径
    let path = Path::new(image_input);
    if path.exists() && path.is_file() {
        // 读取文件并转换为 base64
        let file_bytes = fs::read(image_input)?;
        let mime_type = detect_mime_type_from_path(path)?;
        let base64_data = general_purpose::STANDARD.encode(&file_bytes);
        let data_url = format!("data:{};base64,{}", mime_type, base64_data);
        
        return Ok(ImageContent {
            content_type: "base64".to_string(),
            data: data_url,
            mime_type,
        });
    }
    
    // 如果都不是，尝试作为相对路径处理
    let current_dir = std::env::current_dir()?;
    let full_path = current_dir.join(image_input);
    if full_path.exists() && full_path.is_file() {
        let file_bytes = fs::read(&full_path)?;
        let mime_type = detect_mime_type_from_path(&full_path)?;
        let base64_data = general_purpose::STANDARD.encode(&file_bytes);
        let data_url = format!("data:{};base64,{}", mime_type, base64_data);
        
        return Ok(ImageContent {
            content_type: "base64".to_string(),
            data: data_url,
            mime_type,
        });
    }
    
    // 如果仍然找不到，尝试在 save_directory 中查找
    if let Ok(save_dir) = std::env::var("MCP_SAVE_DIRECTORY") {
        let save_path = Path::new(&save_dir).join(image_input);
        if save_path.exists() && save_path.is_file() {
            let file_bytes = fs::read(&save_path)?;
            let mime_type = detect_mime_type_from_path(&save_path)?;
            let base64_data = general_purpose::STANDARD.encode(&file_bytes);
            let data_url = format!("data:{};base64,{}", mime_type, base64_data);
            
            return Ok(ImageContent {
                content_type: "base64".to_string(),
                data: data_url,
                mime_type,
            });
        }
    }
    
    Err(anyhow!("无法识别的图片输入格式: {}", image_input))
}

/// 在指定的保存目录中查找图片文件
pub fn find_image_in_save_directory(image_input: &str, save_directory: &str) -> Result<ImageContent> {
    let save_path = Path::new(save_directory).join(image_input);
    if save_path.exists() && save_path.is_file() {
        let file_bytes = fs::read(&save_path)?;
        let mime_type = detect_mime_type_from_path(&save_path)?;
        let base64_data = general_purpose::STANDARD.encode(&file_bytes);
        let data_url = format!("data:{};base64,{}", mime_type, base64_data);
        
        return Ok(ImageContent {
            content_type: "base64".to_string(),
            data: data_url,
            mime_type,
        });
    }
    
    Err(anyhow!("在保存目录 '{}' 中找不到图片文件: {}", save_directory, image_input))
}

/// 从 base64 数据中提取 MIME 类型
fn extract_mime_type_from_base64(base64_data: &str) -> Result<String> {
    let mime_part = base64_data
        .split(";base64,")
        .next()
        .ok_or_else(|| anyhow!("无效的base64数据格式"))?;
    
    if mime_part.starts_with("data:image/") {
        Ok(mime_part.to_string())
    } else {
        Err(anyhow!("无效的图像MIME类型: {}", mime_part))
    }
}

/// 从文件路径检测 MIME 类型
fn detect_mime_type_from_path(file_path: &Path) -> Result<String> {
    let extension = file_path
        .extension()
        .and_then(|ext| ext.to_str())
        .ok_or_else(|| anyhow!("无法获取文件扩展名"))?;
    
    let mime_type = match extension.to_lowercase().as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        "svg" => "image/svg+xml",
        _ => "image/*",
    };
    
    Ok(mime_type.to_string())
}

/// 图片内容结构体
#[derive(Debug)]
pub struct ImageContent {
    pub content_type: String,  // "url", "base64", "file"
    pub data: String,          // 实际的数据内容
    pub mime_type: String,     // MIME 类型
} 