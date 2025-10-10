use crate::traits::{DynamicImageTraitInfo, DynamicImageTraitOperation};
use anyhow::{Result, anyhow, bail};
use image::{DynamicImage, ImageEncoder, ImageFormat, codecs::png, load_from_memory_with_format};
use versatiles_core::Blob;

pub fn encode(image: &DynamicImage, speed: Option<u8>) -> Result<Blob> {
	if image.bits_per_value() != 8 {
		bail!("png only supports 8-bit images");
	}

	if image.channel_count() < 1 || image.channel_count() > 4 {
		bail!("png only supports Grey, GreyA, RGB or RGBA");
	}

	let speed = speed.unwrap_or(10).clamp(0, 100);

	use png::{CompressionType, FilterType};
	let (compression_type, filter_type) = match speed {
		0..20 => (CompressionType::Best, FilterType::Adaptive),
		20..40 => (CompressionType::Default, FilterType::Adaptive),
		40..60 => (CompressionType::Default, FilterType::Paeth),
		60..80 => (CompressionType::Default, FilterType::Avg),
		80..90 => (CompressionType::Fast, FilterType::Avg),
		_ => (CompressionType::Fast, FilterType::NoFilter),
	};

	let mut image_ref = image;
	#[allow(unused_assignments)]
	let mut optional_image: Option<DynamicImage> = None;
	if image.has_alpha() && image.is_opaque() {
		let i = image.as_no_alpha()?;
		optional_image = Some(i);
		image_ref = optional_image.as_ref().unwrap();
	}

	let mut buffer: Vec<u8> = Vec::new();
	png::PngEncoder::new_with_quality(&mut buffer, compression_type, filter_type).write_image(
		image_ref.as_bytes(),
		image_ref.width(),
		image_ref.height(),
		image_ref.extended_color_type(),
	)?;

	Ok(Blob::from(buffer))
}

pub fn image2blob(image: &DynamicImage) -> Result<Blob> {
	encode(image, None)
}

pub fn blob2image(blob: &Blob) -> Result<DynamicImage> {
	load_from_memory_with_format(blob.as_slice(), ImageFormat::Png)
		.map_err(|e| anyhow!("Failed to decode PNG image: {e}"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::traits::DynamicImageTraitTest;
	use rstest::rstest;

	/* ---------- Success cases ---------- */
	#[rstest]
	#[case::grey(DynamicImage::new_test_grey(), 0.57)]
	#[case::greya(DynamicImage::new_test_greya(), 0.39)]
	#[case::rgb(DynamicImage::new_test_rgb(), 0.29)]
	#[case::rgba(DynamicImage::new_test_rgba(), 0.33)]
	fn png_ok(#[case] img: DynamicImage, #[case] expected_compression_percent: f64) -> Result<()> {
		let blob = image2blob(&img)?;
		let decoded = blob2image(&blob)?;

		assert_eq!(img.diff(&decoded)?, vec![0.0; img.channel_count() as usize]);

		let compression_percent = ((10_000 * blob.len()) as f64 / img.as_bytes().len() as f64).round() / 100.0;
		assert_eq!(compression_percent, expected_compression_percent);

		Ok(())
	}

	#[rstest]
	#[case::greya(DynamicImage::new_test_greya())]
	#[case::rgba(DynamicImage::new_test_rgba())]
	#[test]
	fn opaque_is_saved_without_alpha(#[case] mut img: DynamicImage) -> Result<()> {
		assert!(img.has_alpha());
		img.make_opaque()?;
		assert!(!blob2image(&encode(&img, Some(80))?)?.has_alpha());
		assert!(!blob2image(&encode(&img, Some(100))?)?.has_alpha());
		Ok(())
	}
}
