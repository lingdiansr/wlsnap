use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    constants::*,
    error::{Result, WlsnapError},
};

/// 顶级配置结构
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub editor: EditorConfig,
    #[serde(default)]
    pub pin: PinConfig,
    #[serde(default)]
    pub scrolling: ScrollingConfig,
    #[serde(default)]
    pub shortcuts: ShortcutsConfig,
    #[serde(default)]
    pub advanced: AdvancedConfig,
}

impl Config {
    /// 加载配置文件。若文件不存在，则创建默认配置并写入。
    pub fn load() -> Result<Self> {
        let config_path = Self::config_path()?;
        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let config: Config = toml::from_str(&content)
                .map_err(|e| WlsnapError::Config(format!("parse error: {e}")))?;
            Ok(config)
        } else {
            let config = Config::default();
            config.save()?;
            Ok(config)
        }
    }

    /// 从指定路径加载配置
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content).map_err(|e| WlsnapError::Config(format!("parse error: {e}")))
    }

    /// 保存配置到默认路径
    pub fn save(&self) -> Result<()> {
        let config_path = Self::config_path()?;
        if let Some(parent) = config_path.parent() {
            std::fs::create_dir_all(parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
            }
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| WlsnapError::Config(format!("serialize error: {e}")))?;
        std::fs::write(&config_path, content)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&config_path, std::fs::Permissions::from_mode(0o600))?;
        }
        Ok(())
    }

    /// 默认配置文件路径
    pub fn config_path() -> Result<PathBuf> {
        let dir = dirs::config_dir()
            .ok_or_else(|| WlsnapError::Config("cannot find config directory".into()))?
            .join(CONFIG_DIR_NAME);
        Ok(dir.join(CONFIG_FILE_NAME))
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeneralConfig {
    pub post_capture: String,
    pub save_dir: String,
    pub filename_template: String,
    pub format: ImageFormat,
    pub jpeg_quality: u8,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            post_capture: "save".into(),
            save_dir: DEFAULT_SAVE_DIR.into(),
            filename_template: DEFAULT_FILENAME_TEMPLATE.into(),
            format: ImageFormat::Png,
            jpeg_quality: DEFAULT_JPEG_QUALITY,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EditorConfig {
    pub stroke_color: String,
    pub stroke_width: f32,
    pub undo_depth: usize,
    pub mosaic_size: u32,
    pub font_family: Option<String>,
    pub font_size: f32,
}

impl Default for EditorConfig {
    fn default() -> Self {
        Self {
            stroke_color: DEFAULT_STROKE_COLOR.into(),
            stroke_width: DEFAULT_STROKE_WIDTH,
            undo_depth: DEFAULT_UNDO_DEPTH,
            mosaic_size: DEFAULT_MOSAIC_SIZE,
            font_family: None,
            font_size: DEFAULT_FONT_SIZE,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PinConfig {
    pub default_scale: f64,
    pub opacity: f64,
    pub show_context_menu: bool,
    pub enable_drag: bool,
    pub enable_scroll_zoom: bool,
}

impl Default for PinConfig {
    fn default() -> Self {
        Self {
            default_scale: 1.0,
            opacity: 0.95,
            show_context_menu: true,
            enable_drag: true,
            enable_scroll_zoom: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScrollingConfig {
    pub auto_scroll_interval_ms: u64,
    pub manual_capture_interval_ms: u64,
    pub stitch_algorithm: StitchAlgorithm,
    pub idle_stop_threshold: usize,
    pub preview_enabled: bool,
}

impl Default for ScrollingConfig {
    fn default() -> Self {
        Self {
            auto_scroll_interval_ms: 500,
            manual_capture_interval_ms: 500,
            stitch_algorithm: StitchAlgorithm::ColumnSampling,
            idle_stop_threshold: 3,
            preview_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ShortcutsConfig {
    pub save: String,
    pub undo: String,
    pub redo: String,
    pub copy: String,
    pub cancel: String,
    pub confirm: String,
    pub switch_mode: String,
    pub zoom_in: String,
    pub zoom_out: String,
    pub reset_zoom: String,
}

impl Default for ShortcutsConfig {
    fn default() -> Self {
        Self {
            save: "Ctrl+S".into(),
            undo: "Ctrl+Z".into(),
            redo: "Ctrl+Shift+Z".into(),
            copy: "Ctrl+C".into(),
            cancel: "Esc".into(),
            confirm: "Enter".into(),
            switch_mode: "Tab".into(),
            zoom_in: "Ctrl++".into(),
            zoom_out: "Ctrl+-".into(),
            reset_zoom: "Ctrl+0".into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AdvancedConfig {
    pub debug: bool,
    pub log_level: String,
    pub include_cursor: bool,
    pub persist_portal_token: bool,
    pub portal_restore_token_path: Option<String>,
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            debug: false,
            log_level: "info".into(),
            include_cursor: false,
            persist_portal_token: true,
            portal_restore_token_path: None,
        }
    }
}

/// 图像保存格式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
pub enum ImageFormat {
    #[default]
    Png,
    Jpeg,
    WebP,
}

impl ImageFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            ImageFormat::Png => "png",
            ImageFormat::Jpeg => "jpeg",
            ImageFormat::WebP => "webp",
        }
    }
}

/// 长截图拼接算法
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
pub enum StitchAlgorithm {
    #[default]
    ColumnSampling,
    Orb,
}

/// 展开路径和文件名模板中的占位符
pub fn expand_placeholders(template: &str, mode: Option<&str>) -> Result<String> {
    let now = chrono::Local::now();
    let mut result = template.to_string();

    if result.contains("{HOME}") {
        let home = dirs::home_dir()
            .ok_or_else(|| WlsnapError::Config("cannot determine home directory".into()))?
            .to_string_lossy()
            .into_owned();
        result = result.replace("{HOME}", &home);
    }

    result = result.replace("{date}", &now.format("%Y-%m-%d").to_string());
    result = result.replace("{time}", &now.format("%H-%M-%S").to_string());
    result = result.replace("{timestamp}", &now.timestamp().to_string());

    if let Some(m) = mode {
        result = result.replace("{mode}", m);
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_serializable() {
        let config = Config::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        assert!(!toml_str.is_empty());
    }

    #[test]
    fn test_expand_placeholders_home() {
        let expanded = expand_placeholders("{HOME}/test", None).unwrap();
        assert!(!expanded.contains("{HOME}"));
    }
}
