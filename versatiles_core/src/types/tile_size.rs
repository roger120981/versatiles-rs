use std::fmt::Debug;

use anyhow::{Result, bail};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TileSize {
	Size256,
	Size512,
}

impl TileSize {
	pub fn new(size: u16) -> Result<Self> {
		match size {
			256 => Ok(Self::Size256),
			512 => Ok(Self::Size512),
			_ => bail!("Invalid tile size: {}. Supported sizes are 256 or 512.", size),
		}
	}

	/// Returns the size of the tile in pixels.
	pub fn size(&self) -> u16 {
		match self {
			TileSize::Size256 => 256,
			TileSize::Size512 => 512,
		}
	}
}

impl Debug for TileSize {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "TileSize({})", self.size())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rstest::rstest;

	#[rstest]
	#[case(256, TileSize::Size256)]
	#[case(512, TileSize::Size512)]
	fn new_accepts_supported_sizes(#[case] size: u16, #[case] expected: TileSize) {
		let ts = TileSize::new(size).expect("expected Ok for supported size");
		assert_eq!(ts, expected);
		assert_eq!(ts.size(), size);
		assert_eq!(format!("{:?}", ts), format!("TileSize({:?})", size));
	}

	#[rstest]
	#[case(0)]
	#[case(1)]
	#[case(255)]
	#[case(257)]
	#[case(511)]
	#[case(513)]
	fn new_rejects_unsupported_sizes(#[case] input: u16) {
		let err = TileSize::new(input).expect_err("expected Err for unsupported size");
		let msg = format!("{}", err);
		assert!(msg.contains("Invalid tile size"));
	}

	#[test]
	fn clone_copy_and_eq_work() {
		let a = TileSize::Size256;
		let b = a; // Copy
		#[allow(clippy::clone_on_copy)]
		let c = a.clone(); // Clone
		assert_eq!(a, b);
		assert_eq!(b, c);
	}
}
