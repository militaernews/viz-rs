//! Needs the ONNX exports in ./models (see README); run with `cargo test -- --ignored`.
use image::{DynamicImage, Rgb, RgbImage};
use viz_core::embed::{ClipTextEmbedder, ClipVisionEmbedder};

fn model(name: &str) -> String {
    format!("{}/../../models/{name}", env!("CARGO_MANIFEST_DIR"))
}

fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
#[ignore]
fn image_and_text_share_an_embedding_space() {
    let vision = ClipVisionEmbedder::load(&model("clip-vision.onnx")).unwrap();
    let text = ClipTextEmbedder::load(&model("clip-text.onnx"), &model("clip-tokenizer.json")).unwrap();
    assert_eq!(vision.dim(), 512);

    let red = DynamicImage::ImageRgb8(RgbImage::from_pixel(320, 240, Rgb([220, 20, 20])));
    let blue = DynamicImage::ImageRgb8(RgbImage::from_pixel(320, 240, Rgb([20, 20, 220])));
    let red_v = vision.embed(&red).unwrap();
    let blue_v = vision.embed(&blue).unwrap();
    let red_t = text.embed_text("a plain red image").unwrap();
    let blue_t = text.embed_text("a plain blue image").unwrap();

    assert_eq!(red_t.len(), 512);
    assert!((dot(&red_v, &red_v) - 1.0).abs() < 1e-4);
    assert!(dot(&red_v, &red_t) > dot(&red_v, &blue_t));
    assert!(dot(&blue_v, &blue_t) > dot(&blue_v, &red_t));
    assert!(text.embed_text(&"tank ".repeat(200)).is_ok());
}
