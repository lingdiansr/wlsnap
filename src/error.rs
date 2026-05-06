use std::path::PathBuf;

#[derive(thiserror::Error, Debug)]
pub enum WlsnapError {
    #[error("Wayland connection failed: {0}")]
    WaylandConnect(String),

    #[error("No suitable capture backend available")]
    NoBackendAvailable,

    #[error("Portal request denied or failed: {0}")]
    PortalDenied(#[from] ashpd::Error),

    #[error("Image processing error: {0}")]
    Image(#[from] image::ImageError),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Clipboard error: {0}")]
    Clipboard(String),

    #[error("Stitching failed: {0}")]
    Stitching(&'static str),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Another instance is already running")]
    AlreadyRunning,

    #[error("External command failed: {0}")]
    ExternalCommand(String),

    #[error("Wayland disconnected")]
    WaylandDisconnected,

    #[error("Save failed: disk full or permission denied at {0}")]
    SaveFailed(PathBuf),

    #[error("No output detected")]
    NoOutputDetected,

    #[error("Font not found: {0}")]
    FontNotFound(String),
}

pub type Result<T> = std::result::Result<T, WlsnapError>;
