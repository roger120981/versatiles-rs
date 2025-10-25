use anyhow::{Result, anyhow, bail};
use byteorder::{LittleEndian as LE, ReadBytesExt, WriteBytesExt};
use std::io::{Cursor, Read};
use versatiles_core::{Blob, TileCompression, TileCoord, TileFormat};
use versatiles_image::{DynamicImage, ImageBuffer};

pub trait CacheValue: Clone {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()>;
	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self>;
}

impl CacheValue for u8 {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		writer.write_u8(*self)?;
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let value = reader.read_u8()?;
		Ok(value)
	}
}

impl CacheValue for u32 {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		writer.write_u32::<LE>(*self)?;
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let value = reader.read_u32::<LE>()?;
		Ok(value)
	}
}

impl CacheValue for String {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		let bytes = self.as_bytes();
		writer.write_u32::<LE>(bytes.len() as u32)?;
		writer.extend_from_slice(bytes);
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let length = reader.read_u32::<LE>()? as usize;
		let mut bytes = vec![0u8; length];
		reader.read_exact(&mut bytes)?;
		String::from_utf8(bytes).map_err(|e| anyhow!(e))
	}
}

impl<T: CacheValue> CacheValue for Vec<T> {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		writer.write_u32::<LE>(self.len() as u32)?;
		for item in self {
			item.write_to_cache(writer)?;
		}
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let length = reader.read_u32::<LE>()? as usize;
		let mut vec = Vec::with_capacity(length);
		for _ in 0..length {
			vec.push(T::read_from_cache(reader)?);
		}
		Ok(vec)
	}
}

impl<A: CacheValue, B: CacheValue> CacheValue for (A, B) {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		self.0.write_to_cache(writer)?;
		self.1.write_to_cache(writer)
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let a = A::read_from_cache(reader)?;
		let b = B::read_from_cache(reader)?;
		Ok((a, b))
	}
}

impl CacheValue for TileCoord {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		writer.write_u8(self.level)?;
		writer.write_u32::<LE>(self.x)?;
		writer.write_u32::<LE>(self.y)?;
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let level = reader.read_u8()?;
		let x = reader.read_u32::<LE>()?;
		let y = reader.read_u32::<LE>()?;
		Ok(TileCoord { x, y, level })
	}
}

impl CacheValue for Blob {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		writer.write_u64::<LE>(self.len())?;
		writer.extend_from_slice(self.as_slice());
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let length = reader.read_u64::<LE>()? as usize;
		let mut bytes = vec![0u8; length];
		reader.read_exact(&mut bytes)?;
		Ok(Blob::from(bytes))
	}
}

impl CacheValue for TileFormat {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		writer.write_u8((*self).into())?;
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let value = reader.read_u8()?;
		TileFormat::try_from(value)
	}
}

impl CacheValue for TileCompression {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		writer.write_u8((*self).into())?;
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let value = reader.read_u8()?;
		TileCompression::try_from(value)
	}
}

impl<V: CacheValue> CacheValue for Option<V> {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		if let Some(value) = self {
			writer.write_u8(1)?; // Indicate presence
			value.write_to_cache(writer)
		} else {
			writer.write_u8(0)?; // Indicate absence
			Ok(())
		}
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let flag = reader.read_u8()?;
		if flag == 1 {
			let value = V::read_from_cache(reader)?;
			Ok(Some(value))
		} else if flag == 0 {
			Ok(None)
		} else {
			bail!("Invalid flag value: {flag}")
		}
	}
}

impl CacheValue for DynamicImage {
	fn write_to_cache(&self, writer: &mut Vec<u8>) -> Result<()> {
		let width = self.width();
		let height = self.height();
		writer.write_u32::<LE>(width)?;
		writer.write_u32::<LE>(height)?;
		let data = self.as_bytes();
		writer.write_u32::<LE>(data.len() as u32)?;
		writer.extend_from_slice(data);
		Ok(())
	}

	fn read_from_cache(reader: &mut Cursor<&[u8]>) -> Result<Self> {
		let width = reader.read_u32::<LE>()?;
		let height = reader.read_u32::<LE>()?;
		let data_length = reader.read_u32::<LE>()? as usize;
		let mut data = vec![0u8; data_length];
		reader.read_exact(&mut data)?;
		let channel_count = data.len() / (width * height) as usize;
		Ok(match channel_count {
			1 => DynamicImage::ImageLuma8(
				ImageBuffer::from_vec(width, height, data)
					.ok_or_else(|| anyhow!("Failed to create Luma8 image buffer with provided data"))?,
			),
			2 => DynamicImage::ImageLumaA8(
				ImageBuffer::from_vec(width, height, data)
					.ok_or_else(|| anyhow!("Failed to create LumaA8 image buffer with provided data"))?,
			),
			3 => DynamicImage::ImageRgb8(
				ImageBuffer::from_vec(width, height, data)
					.ok_or_else(|| anyhow!("Failed to create RGB8 image buffer with provided data"))?,
			),
			4 => DynamicImage::ImageRgba8(
				ImageBuffer::from_vec(width, height, data)
					.ok_or_else(|| anyhow!("Failed to create RGBA8 image buffer with provided data"))?,
			),
			_ => bail!("Unsupported channel count: {channel_count}"),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	fn roundtrip<T>(value: T)
	where
		T: CacheValue + PartialEq + core::fmt::Debug,
	{
		let mut buf = vec![];
		value.write_to_cache(&mut buf).unwrap();

		let mut cursor = Cursor::new(buf.as_slice());
		assert_eq!(cursor.position(), 0);

		let decoded = T::read_from_cache(&mut cursor).unwrap();
		assert_eq!(decoded, value);
		assert_eq!(cursor.position(), buf.len() as u64);
	}

	#[rstest]
	#[case(vec![])]
	#[case(vec![0])]
	#[case(vec![0, 1, 2, 3, 4, 5])]
	#[case(vec![255; 1024])] // 1KB of 0xFF bytes
	#[case(vec![0, 255, 128, 10, 200, 0])] // include non-UTF8 bytes to ensure raw bytes are preserved
	fn vec_u8_roundtrips_various_payloads(#[case] payload: Vec<u8>) {
		roundtrip::<Vec<u8>>(payload);
	}

	#[rstest]
	#[case("")] // empty string
	#[case("hello world")] // simple ASCII
	#[case("Grüße 🌍 — こんにちは")] // Unicode
	#[case("naïve café")] // Unicode with accents
	#[case("a".repeat(1000))] // long string
	fn string_roundtrips_ascii_and_unicode(#[case] payload: String) {
		roundtrip::<String>(payload.to_string());
	}

	#[test]
	fn string_from_cache_buffer_panics_on_invalid_utf8() {
		// Construct a buffer that is not valid UTF-8 (single 0xFF byte)
		let invalid = [0xFFu8, 0xFEu8, 0x00u8];
		assert!(String::read_from_cache(&mut Cursor::new(&invalid)).is_err());
	}

	#[test]
	fn u8_roundtrip() {
		roundtrip::<u8>(0);
		roundtrip::<u8>(1);
		roundtrip::<u8>(255);
	}

	#[test]
	fn u32_roundtrip() {
		roundtrip::<u32>(0);
		roundtrip::<u32>(1);
		roundtrip::<u32>(u32::MAX);
	}

	#[test]
	fn tuple_roundtrip() {
		let value: (u32, String) = (42, "life".to_string());
		roundtrip::<(u32, String)>(value);
	}

	#[test]
	fn tilecoord_roundtrip() {
		let tc = TileCoord {
			x: 123456,
			y: 654321,
			level: 7,
		};
		roundtrip::<TileCoord>(tc);
	}

	#[test]
	fn blob_roundtrip() {
		let data: Vec<u8> = (0..=255).collect();
		let blob = Blob::from(data);
		roundtrip::<Blob>(blob);
	}

	#[test]
	fn option_roundtrip_some_none() {
		let some_v: Option<u32> = Some(1234567890);
		roundtrip::<Option<u32>>(some_v);

		let none_v: Option<String> = None;
		roundtrip::<Option<String>>(none_v);
	}

	#[test]
	fn option_from_cache_errors_on_invalid_flag() {
		// Prepare a buffer with an invalid presence flag (2)
		let buf = vec![2u8];
		let mut cursor = Cursor::new(buf.as_slice());
		let res = <Option<u8>>::read_from_cache(&mut cursor);
		assert!(res.is_err());
	}

	#[test]
	fn tileformat_accepts_all_valid_discriminants() {
		// For every u8 that maps to a valid TileFormat, verify roundtrip remains stable.
		for v in 0u8..=u8::MAX {
			if let Ok(tf) = TileFormat::try_from(v) {
				// encode
				let mut buf = vec![];
				tf.write_to_cache(&mut buf).unwrap();
				// decode
				let mut cur = Cursor::new(buf.as_slice());
				let tf2 = TileFormat::read_from_cache(&mut cur).unwrap();
				assert_eq!(tf2, tf);
				assert_eq!(cur.position(), buf.len() as u64);
			}
		}
	}

	#[test]
	fn tilecompression_accepts_all_valid_discriminants() {
		for v in 0u8..=u8::MAX {
			if let Ok(tc) = TileCompression::try_from(v) {
				let mut buf = vec![];
				tc.write_to_cache(&mut buf).unwrap();
				let mut cur = Cursor::new(buf.as_slice());
				let tc2 = TileCompression::read_from_cache(&mut cur).unwrap();
				assert_eq!(tc2, tc);
				assert_eq!(cur.position(), buf.len() as u64);
			}
		}
	}

	fn make_image_dynamic(kind: &str) -> DynamicImage {
		let (w, h) = (2u32, 2u32);
		match kind {
			"luma" => {
				let data = vec![10u8, 20, 30, 40]; // 4 bytes
				DynamicImage::ImageLuma8(ImageBuffer::from_vec(w, h, data).unwrap())
			}
			"lumaa" => {
				let data = vec![10u8, 1, 20, 2, 30, 3, 40, 4]; // 8 bytes
				DynamicImage::ImageLumaA8(ImageBuffer::from_vec(w, h, data).unwrap())
			}
			"rgb" => {
				let data = vec![
					255, 0, 0, // R
					0, 255, 0, // G
					0, 0, 255, // B
					10, 20, 30, // misc
				]; // 12 bytes
				DynamicImage::ImageRgb8(ImageBuffer::from_vec(w, h, data).unwrap())
			}
			"rgba" => {
				let data = vec![255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 10, 20, 30, 0]; // 16 bytes
				DynamicImage::ImageRgba8(ImageBuffer::from_vec(w, h, data).unwrap())
			}
			_ => unreachable!(),
		}
	}

	#[rstest]
	#[case("luma")]
	#[case("lumaa")]
	#[case("rgb")]
	#[case("rgba")]
	fn dynamic_image_roundtrips(#[case] kind: &str) {
		let img = make_image_dynamic(kind);
		roundtrip::<DynamicImage>(img);
	}
}
