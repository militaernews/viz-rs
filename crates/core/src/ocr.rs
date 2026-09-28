use anyhow::{Result, bail};
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;
use tokio::time::timeout;

const OCR_TIMEOUT: Duration = Duration::from_secs(30);

/// Best-effort OCR through the `tesseract` CLI. Disabled when no binary is configured.
#[derive(Clone)]
pub struct Ocr {
    tesseract_bin: Option<String>,
    languages: String,
}

impl Ocr {
    pub fn new(tesseract_bin: Option<String>, languages: String) -> Self {
        Self { tesseract_bin, languages }
    }

    pub fn is_enabled(&self) -> bool {
        self.tesseract_bin.is_some()
    }

    /// `image_bytes` is any format tesseract reads (JPEG/PNG/WebP/GIF).
    pub async fn recognize(&self, image_bytes: &[u8]) -> Result<Option<String>> {
        let Some(bin) = &self.tesseract_bin else {
            return Ok(None);
        };
        let dir = tempfile::tempdir()?;
        let input = dir.path().join("ocr-input");
        tokio::fs::write(&input, image_bytes).await?;

        let output = Command::new(bin)
            .arg(&input)
            .arg("stdout")
            .args(["-l", &self.languages])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output();
        let output = match timeout(OCR_TIMEOUT, output).await {
            Ok(result) => result?,
            Err(_) => bail!("tesseract timed out"),
        };
        if !output.status.success() {
            bail!("tesseract exited with {}", output.status);
        }
        let text = String::from_utf8_lossy(&output.stdout).split_whitespace().collect::<Vec<_>>().join(" ");
        Ok((!text.is_empty()).then_some(text))
    }
}
