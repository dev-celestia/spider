use std::path::{Path, PathBuf};
use crate::types::{PageIR, StorageExporter};

/// Builder for constructing [`FileStorageExporter`] instances.
#[derive(Default)]
pub struct FileStorageExporterBuilder {
    output_dir: Option<PathBuf>,
}

impl FileStorageExporterBuilder {
    /// Creates a new `FileStorageExporterBuilder`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the output directory path.
    pub fn output_dir<P: AsRef<Path>>(mut self, dir: P) -> Self {
        self.output_dir = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Builds the [`FileStorageExporter`].
    pub fn build(self) -> FileStorageExporter {
        let dir = self.output_dir.unwrap_or_else(|| PathBuf::from("out"));
        FileStorageExporter::new(dir)
    }
}

/// Phase 4 File Storage Exporter.
///
/// Saves extracted `PageIR` payloads as Markdown `.md` files to disk.
/// Defaults to outputting in `./out` directory, but can be customized to any output path.
#[derive(Debug, Clone)]
pub struct FileStorageExporter {
    output_dir: PathBuf,
}

impl FileStorageExporter {
    /// Creates a new `FileStorageExporterBuilder` instance.
    pub fn builder() -> FileStorageExporterBuilder {
        FileStorageExporterBuilder::default()
    }

    /// Creates a new `FileStorageExporter` targeting a custom output directory path.
    ///
    /// # Examples
    /// ```
    /// use celestia_spider::FileStorageExporter;
    ///
    /// let custom_exporter = FileStorageExporter::new("./custom_output");
    /// ```
    pub fn new<P: AsRef<Path>>(output_dir: P) -> Self {
        Self {
            output_dir: output_dir.as_ref().to_path_buf(),
        }
    }

    /// Returns the target output directory path.
    pub fn output_dir(&self) -> &Path {
        &self.output_dir
    }
}

impl Default for FileStorageExporter {
    /// Creates a `FileStorageExporter` with the default output directory `./out`.
    ///
    /// # Examples
    /// ```
    /// use celestia_spider::FileStorageExporter;
    ///
    /// let default_exporter = FileStorageExporter::default();
    /// assert_eq!(default_exporter.output_dir().to_str().unwrap(), "out");
    /// ```
    fn default() -> Self {
        Self::new("out")
    }
}

#[async_trait::async_trait]
impl StorageExporter for FileStorageExporter {
    async fn export(&self, ir: &PageIR) -> Result<(), String> {
        tokio::fs::create_dir_all(&self.output_dir)
            .await
            .map_err(|e| format!("Failed to create output directory '{}': {e}", self.output_dir.display()))?;

        let sanitized = ir
            .url
            .replace("https://", "")
            .replace("http://", "")
            .replace('/', "_");

        let filename = format!("{}.md", sanitized);
        let file_path = self.output_dir.join(filename);

        tokio::fs::write(&file_path, &ir.markdown_ir)
            .await
            .map_err(|e| format!("Failed to write IR file to '{}': {e}", file_path.display()))?;

        println!("[Phase 4] Saved to disk: {}", file_path.display());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_file_storage_exporter_custom_dir() {
        let custom_dir = std::env::temp_dir().join("celestia_spider_test_export");
        let exporter = FileStorageExporter::new(&custom_dir);

        let sample_ir = PageIR {
            url: "https://example.com/test".to_string(),
            title: "Test Title".to_string(),
            markdown_ir: "# Sample Header\nTest content.".to_string(),
        };

        exporter.export(&sample_ir).await.unwrap();

        let expected_file = custom_dir.join("example.com_test.md");
        assert!(expected_file.exists());

        let written_content = tokio::fs::read_to_string(&expected_file).await.unwrap();
        assert_eq!(written_content, "# Sample Header\nTest content.");

        let _ = tokio::fs::remove_dir_all(&custom_dir).await;
    }

    #[test]
    fn test_default_output_dir() {
        let exporter = FileStorageExporter::default();
        assert_eq!(exporter.output_dir(), Path::new("out"));

        let builder_exporter = FileStorageExporter::builder().output_dir("custom_path").build();
        assert_eq!(builder_exporter.output_dir(), Path::new("custom_path"));
    }
}
