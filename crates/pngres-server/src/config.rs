use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub data_dir: PathBuf,
    pub max_upload_bytes: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_owned(),
            port: 6969,
            data_dir: PathBuf::from("./herd"),
            max_upload_bytes: 2 * 1024 * 1024 * 1024,
        }
    }
}
