use mime_guess::Mime;
use std::io::ErrorKind;
use std::path::PathBuf;
use tokio::fs::{self, File};

use super::{StateError, StateResult};

#[derive(Clone, Debug)]
pub struct FileSystem {
  pub root: PathBuf,
}

impl FileSystem {
  pub async fn init() -> StateResult<Self> {
    tracing::debug!("initializing file system");
    let root = dirs::data_dir()
      .unwrap_or("/home/superbird/.local/share".into())
      .join("bridgething");

    if !root.exists() {
      tokio::fs::create_dir_all(&root).await?;
    }

    Ok(Self { root })
  }

  pub async fn save_file<P: AsRef<str>>(&self, path: P, data: Vec<u8>) -> StateResult<()> {
    let path = path.as_ref();
    tracing::debug!("saving file: {}", path);

    let full_path = self.root.join(path);
    if let Some(parent) = full_path.parent() {
      fs::create_dir_all(parent).await?;
    }

    fs::write(full_path, data).await?;
    Ok(())
  }

  pub async fn delete_file<P: AsRef<str>>(&self, path: P) -> StateResult<()> {
    let path = path.as_ref();
    tracing::debug!("deleting file: {}", path);
    let full_path = self.root.join(path);
    match fs::remove_file(&full_path).await {
      Ok(_) => Ok(()),
      Err(e) if e.kind() == ErrorKind::NotFound => Err(StateError::FileNotFound(path.to_string())),
      Err(e) => Err(StateError::Io(e)),
    }
  }

  pub async fn read_file<P: AsRef<str>>(&self, path: P) -> StateResult<(Vec<u8>, Mime)> {
    let path = path.as_ref();
    tracing::debug!("reading file: {}", path);
    let full_path = self.root.join(path);
    let data = fs::read(&full_path).await.map_err(|e| {
      if e.kind() == ErrorKind::NotFound {
        StateError::FileNotFound(path.to_string())
      } else {
        StateError::Io(e)
      }
    })?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Ok((data, mime))
  }

  pub async fn get_file<P: AsRef<str>>(&self, path: P) -> StateResult<(File, Mime)> {
    let path = path.as_ref();
    tracing::debug!("getting file: {}", path);
    let full_path = self.root.join(path);
    let file = File::open(&full_path).await.map_err(|e| {
      if e.kind() == ErrorKind::NotFound {
        StateError::FileNotFound(path.to_string())
      } else {
        StateError::Io(e)
      }
    })?;
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    Ok((file, mime))
  }

  pub async fn file_exists<P: AsRef<str>>(&self, path: P) -> StateResult<bool> {
    let path = path.as_ref();
    tracing::debug!("checking if file exists: {}", path);
    let full_path = self.root.join(path);
    let exists = full_path.exists();
    Ok(exists)
  }

  /// TODO: this is comically inefficient :)
  pub async fn list_files(&self) -> StateResult<Vec<String>> {
    tracing::debug!("listing files");
    let mut files = Vec::new();
    list_dir_recursive(&self.root, &self.root, &mut files)
      .await
      .map_err(StateError::Io)?;
    Ok(files)
  }
}

// recursive helper to traverse filesystem under root
async fn list_dir_recursive(dir: &PathBuf, root: &PathBuf, files: &mut Vec<String>) -> std::io::Result<()> {
  let mut read_dir = fs::read_dir(dir).await?;
  while let Some(entry) = read_dir.next_entry().await? {
    let path = entry.path();
    if path.is_dir() {
      Box::pin(list_dir_recursive(&path, root, files)).await?;
    } else if let Ok(rel) = path.strip_prefix(root) {
      files.push(rel.to_string_lossy().to_string());
    }
  }
  Ok(())
}
