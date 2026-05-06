pub const APP_NAME: &str = "wlsnap";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// 配置文件目录名（位于 ~/.config/ 下）
pub const CONFIG_DIR_NAME: &str = "wlsnap";
/// 配置文件名
pub const CONFIG_FILE_NAME: &str = "config.toml";

/// 缓存目录名（位于 ~/.cache/ 下）
pub const CACHE_DIR_NAME: &str = "wlsnap";
/// 单实例 Unix socket 名
pub const INSTANCE_SOCKET_NAME: &str = "instance.sock";
/// Portal restore token 存储文件名
pub const PORTAL_TOKEN_FILE_NAME: &str = "portal_token.json";

/// 默认保存目录模板
pub const DEFAULT_SAVE_DIR: &str = "{HOME}/Pictures/Screenshots";
/// 默认文件名模板
pub const DEFAULT_FILENAME_TEMPLATE: &str = "wlsnap_{date}_{time}_{mode}";
/// 默认 JPEG 质量
pub const DEFAULT_JPEG_QUALITY: u8 = 90;
/// 默认撤销栈深度
pub const DEFAULT_UNDO_DEPTH: usize = 50;
/// 默认马赛克块大小
pub const DEFAULT_MOSAIC_SIZE: u32 = 10;
/// 默认字体大小
pub const DEFAULT_FONT_SIZE: f32 = 16.0;
/// 默认描边粗细
pub const DEFAULT_STROKE_WIDTH: f32 = 3.0;
/// 默认描边颜色
pub const DEFAULT_STROKE_COLOR: &str = "#FF5722";
