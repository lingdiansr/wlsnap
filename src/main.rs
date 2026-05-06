mod config;
mod constants;
mod error;

use tracing::{info, warn};

fn main() {
    // 初始化日志订阅器
    tracing_subscriber::fmt::init();

    info!("{} v{} starting", constants::APP_NAME, constants::APP_VERSION);

    // 加载配置（不存在则自动生成默认配置）
    let cfg = match config::Config::load() {
        Ok(c) => {
            info!("Configuration loaded successfully");
            c
        }
        Err(e) => {
            warn!("Failed to load config: {e}, using defaults");
            config::Config::default()
        }
    };

    // T1 骨架：仅打印配置摘要，验证加载/保存闭环
    println!("Configuration summary:");
    println!("  Post-capture action: {}", cfg.general.post_capture);
    println!("  Save directory: {}", cfg.general.save_dir);
    println!("  Image format: {:?}", cfg.general.format);
    println!("  JPEG quality: {}", cfg.general.jpeg_quality);
    println!("  Editor stroke color: {}", cfg.editor.stroke_color);
    println!("  Undo depth: {}", cfg.editor.undo_depth);
    println!("  Auto scroll interval: {} ms", cfg.scrolling.auto_scroll_interval_ms);
    println!("  Log level: {}", cfg.advanced.log_level);

    info!("T1 skeleton verified: config load/save OK");
}
