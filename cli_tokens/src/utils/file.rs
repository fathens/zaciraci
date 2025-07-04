use anyhow::Result;
use serde::Serialize;
use std::fs;
use std::path::Path;
use tokio::fs as async_fs;

pub fn ensure_directory_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        fs::create_dir_all(path)?;
    }
    Ok(())
}

pub async fn write_json_file<T: Serialize>(path: &Path, data: &T) -> Result<()> {
    let json = serde_json::to_string_pretty(data)?;
    async_fs::write(path, json).await?;
    Ok(())
}

pub async fn file_exists(path: &Path) -> bool {
    async_fs::metadata(path).await.is_ok()
}

pub fn sanitize_filename(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}
