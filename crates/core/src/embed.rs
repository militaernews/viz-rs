use anyhow::{Context, Result, anyhow, bail};
use image::DynamicImage;
use image::imageops::FilterType;
use ort::session::Session;
use ort::session::builder::GraphOptimizationLevel;
use ort::value::Tensor;
use std::sync::{Mutex, MutexGuard};

// Values from the model's preprocessor_config.json (openai/clip-vit-base-patch32).
const CLIP_SIZE: u32 = 224;
const CLIP_MEAN: [f32; 3] = [0.481_454_66, 0.457_827_5, 0.408_210_73];
const CLIP_STD: [f32; 3] = [0.268_629_54, 0.261_302_6, 0.275_777_1];
const CLIP_MAX_TOKENS: usize = 77;

/// CLIP image tower. Load once per process and share via `Arc`; inference is serialized
/// because `ort::Session::run` needs exclusive access.
pub struct ClipVisionEmbedder {
    session: Mutex<Session>,
    dim: usize,
}

impl ClipVisionEmbedder {
    pub fn load(model_path: &str) -> Result<Self> {
        let session = load_session(model_path)?;
        require_io(&session, &["pixel_values"], "image_embeds", model_path)?;
        let mut embedder = Self { session: Mutex::new(session), dim: 0 };
        embedder.dim = embedder.embed(&DynamicImage::new_rgb8(CLIP_SIZE, CLIP_SIZE))?.len();
        Ok(embedder)
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    /// Returns an L2-normalized embedding.
    pub fn embed(&self, img: &DynamicImage) -> Result<Vec<f32>> {
        let pixels = Tensor::from_array(([1usize, 3, CLIP_SIZE as usize, CLIP_SIZE as usize], preprocess_clip(img)))?;
        let mut session = lock(&self.session);
        let outputs = session.run(ort::inputs!["pixel_values" => pixels])?;
        let (_, embedding) = outputs["image_embeds"].try_extract_tensor::<f32>()?;
        Ok(l2_normalize(embedding.to_vec()))
    }
}

/// CLIP text tower, producing vectors in the same space as [`ClipVisionEmbedder`].
pub struct ClipTextEmbedder {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    wants_attention_mask: bool,
}

impl ClipTextEmbedder {
    pub fn load(model_path: &str, tokenizer_path: &str) -> Result<Self> {
        let session = load_session(model_path)?;
        require_io(&session, &["input_ids"], "text_embeds", model_path)?;
        let wants_attention_mask = session.inputs().iter().any(|i| i.name() == "attention_mask");
        let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path)
            .map_err(|e| anyhow!("loading tokenizer {tokenizer_path}: {e}"))?;
        Ok(Self { session: Mutex::new(session), tokenizer, wants_attention_mask })
    }

    /// Returns an L2-normalized embedding.
    pub fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let encoding = self.tokenizer.encode(text, true).map_err(|e| anyhow!("tokenize failed: {e}"))?;
        let ids = truncate_tokens(encoding.get_ids().iter().map(|&id| i64::from(id)).collect());
        let len = ids.len();

        let input_ids = Tensor::from_array(([1usize, len], ids))?;
        let mut session = lock(&self.session);
        let outputs = if self.wants_attention_mask {
            let mask = Tensor::from_array(([1usize, len], vec![1i64; len]))?;
            session.run(ort::inputs!["input_ids" => input_ids, "attention_mask" => mask])?
        } else {
            session.run(ort::inputs!["input_ids" => input_ids])?
        };
        let (_, embedding) = outputs["text_embeds"].try_extract_tensor::<f32>()?;
        Ok(l2_normalize(embedding.to_vec()))
    }
}

fn load_session(model_path: &str) -> Result<Session> {
    let configure = |e: &dyn std::fmt::Display| anyhow!("configuring ONNX session: {e}");
    Session::builder()
        .map_err(|e| configure(&e))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| configure(&e))?
        .with_intra_threads(4)
        .map_err(|e| configure(&e))?
        .commit_from_file(model_path)
        .with_context(|| format!("loading ONNX model {model_path}"))
}

fn require_io(session: &Session, inputs: &[&str], output: &str, model_path: &str) -> Result<()> {
    let has_input = |name: &str| session.inputs().iter().any(|i| i.name() == name);
    let has_output = session.outputs().iter().any(|o| o.name() == output);
    if inputs.iter().all(|i| has_input(i)) && has_output {
        return Ok(());
    }
    let names = |outlets: &[ort::value::Outlet]| outlets.iter().map(|o| o.name().to_string()).collect::<Vec<_>>();
    bail!(
        "{model_path}: expected inputs {inputs:?} and output {output:?}, model has inputs {:?} and outputs {:?}",
        names(session.inputs()),
        names(session.outputs())
    )
}

fn lock(session: &Mutex<Session>) -> MutexGuard<'_, Session> {
    // A panic mid-inference leaves no state in the session worth distrusting.
    session.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// CLIP keeps the start and end-of-text tokens; the text is truncated in between.
fn truncate_tokens(mut ids: Vec<i64>) -> Vec<i64> {
    if ids.len() > CLIP_MAX_TOKENS {
        let eot = ids[ids.len() - 1];
        ids.truncate(CLIP_MAX_TOKENS - 1);
        ids.push(eot);
    }
    ids
}

/// Resize shortest edge to 224 (bicubic), center-crop 224x224, scale to 0..1,
/// normalize with CLIP mean/std, HWC -> CHW.
fn preprocess_clip(img: &DynamicImage) -> Vec<f32> {
    let (w, h) = (img.width().max(1), img.height().max(1));
    let scale = CLIP_SIZE as f32 / w.min(h) as f32;
    let new_w = ((w as f32 * scale).round() as u32).max(CLIP_SIZE);
    let new_h = ((h as f32 * scale).round() as u32).max(CLIP_SIZE);
    let resized = img.resize_exact(new_w, new_h, FilterType::CatmullRom).to_rgb8();
    let (x0, y0) = ((new_w - CLIP_SIZE) / 2, (new_h - CLIP_SIZE) / 2);
    let cropped = image::imageops::crop_imm(&resized, x0, y0, CLIP_SIZE, CLIP_SIZE).to_image();

    let plane = (CLIP_SIZE * CLIP_SIZE) as usize;
    let mut out = vec![0f32; 3 * plane];
    for (x, y, pixel) in cropped.enumerate_pixels() {
        let i = (y * CLIP_SIZE + x) as usize;
        for c in 0..3 {
            out[c * plane + i] = (f32::from(pixel[c]) / 255.0 - CLIP_MEAN[c]) / CLIP_STD[c];
        }
    }
    out
}

fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in v.iter_mut() {
            *x /= norm;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_shape_and_normalization() {
        let white = DynamicImage::ImageRgb8(image::RgbImage::from_pixel(640, 360, image::Rgb([255, 255, 255])));
        let out = preprocess_clip(&white);
        assert_eq!(out.len(), 3 * 224 * 224);
        for c in 0..3 {
            let expected = (1.0 - CLIP_MEAN[c]) / CLIP_STD[c];
            assert!((out[c * 224 * 224 + 1234] - expected).abs() < 1e-4);
        }
        assert_eq!(preprocess_clip(&DynamicImage::new_rgb8(3, 1000)).len(), 3 * 224 * 224);
    }

    #[test]
    fn truncation_keeps_end_token() {
        let ids: Vec<i64> = (0..100).collect();
        let t = truncate_tokens(ids);
        assert_eq!(t.len(), CLIP_MAX_TOKENS);
        assert_eq!(t[0], 0);
        assert_eq!(*t.last().unwrap(), 99);
        assert_eq!(truncate_tokens(vec![1, 2]), vec![1, 2]);
    }

    #[test]
    fn normalizes_to_unit_length() {
        let v = l2_normalize(vec![3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6 && (v[1] - 0.8).abs() < 1e-6);
        assert_eq!(l2_normalize(vec![0.0, 0.0]), vec![0.0, 0.0]);
    }
}
