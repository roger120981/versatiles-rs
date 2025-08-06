//! Utilities for constructing and applying tile traversal strategies.
//!
//! Defines the `Traversal` type, which combines a traversal order and block size range,
//! and provides methods to generate ordered tile bounding boxes from a pyramid.

use crate::{
	TileBBox, TileBBoxPyramid,
	traversal::{TraversalOrder, TraversalSize},
};
use anyhow::Result;

#[derive(Clone, PartialEq)]
/// Represents a traversal strategy for iterating over tile bounding boxes.
///
/// A `Traversal` combines a `TraversalOrder` (ordering of blocks)
/// and a `TraversalSize` (range of block sizes) to generate
/// an ordered sequence of `TileBBox` instances from a `TileBBoxPyramid`.
pub struct Traversal {
	order: TraversalOrder,
	size: TraversalSize,
}

impl Traversal {
	/// Create a new `Traversal` with the given block ordering and size range.
	///
	/// # Parameters
	/// - `order`: the `TraversalOrder` (e.g., depth-first, Hilbert).
	/// - `min_size`: minimum block size in tiles (power of two).
	/// - `max_size`: maximum block size in tiles (power of two).
	///
	/// # Errors
	/// Returns an error if size parameters are invalid (not powers of two or out of range).
	pub fn new(order: TraversalOrder, min_size: u32, max_size: u32) -> Result<Traversal> {
		Ok(Traversal {
			order,
			size: TraversalSize::new(min_size, max_size)?,
		})
	}

	/// Create a `Traversal` with any order and the specified size range.
	///
	/// Uses `TraversalOrder::AnyOrder` with the same size validation as `new`.
	pub fn new_any_size(min_size: u32, max_size: u32) -> Result<Traversal> {
		Ok(Traversal {
			order: TraversalOrder::AnyOrder,
			size: TraversalSize::new(min_size, max_size)?,
		})
	}

	/// Create a `Traversal` with any order and the default size range (1 to 2^31).
	pub const fn new_any() -> Self {
		Traversal {
			order: TraversalOrder::AnyOrder,
			size: TraversalSize::new_default(),
		}
	}

	/// Return the maximum block size in tiles for this `Traversal`.
	///
	/// # Errors
	/// Returns an error if the size range is invalid.
	pub fn get_max_size(&self) -> Result<u32> {
		self.size.max_size()
	}

	/// Access the `TraversalOrder` (block ordering strategy).
	pub fn order(&self) -> &TraversalOrder {
		&self.order
	}

	/// Modify this `Traversal` to be the intersection with another.
	///
	/// Combines size and order; errors if the order or sizes cannot intersect.
	pub fn intersect(&mut self, other: &Traversal) -> Result<()> {
		self.order.intersect(&other.order)?;
		self.size.intersect(&other.size)?;
		Ok(())
	}

	/// Return a new `Traversal` that is the intersection of this and another, without modifying either.
	pub fn get_intersected(&self, other: &Traversal) -> Result<Traversal> {
		let mut result = self.clone();
		result.intersect(other)?;
		Ok(result)
	}

	/// Traverse the tile pyramid, returning all `TileBBox` in traversal order.
	///
	/// Generates bounding boxes at each level, groups them by block size,
	/// applies the traversal order, and returns a flat vector.
	///
	/// # Parameters
	/// - `pyramid`: the `TileBBoxPyramid` defining the tile grid per zoom level.
	///
	/// # Errors
	/// Returns an error if size computation or ordering fails.
	pub fn traverse_pyramid(&self, pyramid: &TileBBoxPyramid) -> Result<Vec<TileBBox>> {
		let size = self.get_max_size()?;
		let mut bboxes: Vec<TileBBox> = pyramid.level_bbox.iter().flat_map(|b| b.iter_bbox_grid(size)).collect();
		self.order.sort_bboxes(&mut bboxes, size);
		Ok(bboxes)
	}

	pub const ANY: Self = Self::new_any();
}

impl Default for Traversal {
	fn default() -> Self {
		Traversal::ANY
	}
}

impl std::fmt::Debug for Traversal {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		if self.size.is_empty() {
			write!(f, "Traversal({:?}, but no suitable block size)", self.order)
		} else {
			write!(
				f,
				"Traversal({:?}, min-size: {}, max-size: {})",
				self.order,
				self
					.size
					.min_size()
					.map(|s| s.to_string())
					.unwrap_or_else(|e| e.to_string()),
				self
					.size
					.max_size()
					.map(|s| s.to_string())
					.unwrap_or_else(|e| e.to_string())
			)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::GeoBBox;
	use enumset::EnumSet;

	#[test]
	fn test_traverse_pyramid() {
		fn test(traversal_order: TraversalOrder, size: u32, bbox: [i16; 4], min_level: u8, max_level: u8) -> Vec<String> {
			let pyramid = TileBBoxPyramid::from_geo_bbox(min_level, max_level, &GeoBBox::from(&bbox));
			let traversal = Traversal {
				order: traversal_order,
				size: TraversalSize::new(1, size).unwrap(),
			};
			traversal
				.traverse_pyramid(&pyramid)
				.unwrap()
				.iter()
				.map(|b| b.as_string())
				.collect()
		}

		use TraversalOrder::*;
		for order in EnumSet::all() {
			match order {
				AnyOrder => {
					assert_eq!(
						test(order, 256, [-180, -90, 180, 90], 0, 5),
						[
							"0:[0,0,0,0]",
							"1:[0,0,1,1]",
							"2:[0,0,3,3]",
							"3:[0,0,7,7]",
							"4:[0,0,15,15]",
							"5:[0,0,31,31]"
						]
					);
					assert_eq!(
						test(order, 16, [-180, -90, 180, 90], 4, 5),
						[
							"4:[0,0,15,15]",
							"5:[0,0,15,15]",
							"5:[16,0,31,15]",
							"5:[0,16,15,31]",
							"5:[16,16,31,31]"
						]
					);
				}
				DepthFirst => {
					assert_eq!(
						test(order, 16, [-170, -60, 160, 70], 4, 6),
						[
							"6:[1,14,15,15]",
							"6:[16,14,31,15]",
							"6:[1,16,15,31]",
							"6:[16,16,31,31]",
							"5:[0,7,15,15]",
							"6:[32,14,47,15]",
							"6:[48,14,60,15]",
							"6:[32,16,47,31]",
							"6:[48,16,60,31]",
							"5:[16,7,30,15]",
							"6:[1,32,15,45]",
							"6:[16,32,31,45]",
							"5:[0,16,15,22]",
							"6:[32,32,47,45]",
							"6:[48,32,60,45]",
							"5:[16,16,30,22]",
							"4:[0,3,15,11]",
						]
					);
					assert_eq!(
						test(order, 32, [-170, -60, 160, 70], 4, 6),
						[
							"6:[1,14,31,31]",
							"6:[32,14,60,31]",
							"6:[1,32,31,45]",
							"6:[32,32,60,45]",
							"5:[0,7,30,22]",
							"4:[0,3,15,11]"
						]
					);
					assert_eq!(
						test(order, 256, [-170, -60, 160, 70], 6, 10),
						[
							"10:[28,229,255,255]",
							"10:[256,229,511,255]",
							"10:[28,256,255,511]",
							"10:[256,256,511,511]",
							"9:[14,114,255,255]",
							"10:[512,229,767,255]",
							"10:[768,229,967,255]",
							"10:[512,256,767,511]",
							"10:[768,256,967,511]",
							"9:[256,114,483,255]",
							"10:[28,512,255,726]",
							"10:[256,512,511,726]",
							"9:[14,256,255,363]",
							"10:[512,512,767,726]",
							"10:[768,512,967,726]",
							"9:[256,256,483,363]",
							"8:[7,57,241,181]",
							"7:[3,28,120,90]",
							"6:[1,14,60,45]"
						]
					)
				}
				PMTiles => {
					assert_eq!(
						test(order, 64, [-170, -60, 160, 70], 6, 8),
						[
							"6:[1,14,60,45]",
							"7:[3,28,63,63]",
							"7:[3,64,63,90]",
							"7:[64,64,120,90]",
							"7:[64,28,120,63]",
							"8:[7,57,63,63]",
							"8:[64,57,127,63]",
							"8:[64,64,127,127]",
							"8:[7,64,63,127]",
							"8:[7,128,63,181]",
							"8:[64,128,127,181]",
							"8:[128,128,191,181]",
							"8:[192,128,241,181]",
							"8:[192,64,241,127]",
							"8:[128,64,191,127]",
							"8:[128,57,191,63]",
							"8:[192,57,241,63]"
						]
					);
					assert_eq!(
						test(order, 128, [-170, -60, 160, 70], 6, 8),
						[
							"6:[1,14,60,45]",
							"7:[3,28,120,90]",
							"8:[7,57,127,127]",
							"8:[7,128,127,181]",
							"8:[128,128,241,181]",
							"8:[128,57,241,127]"
						]
					)
				}
			}
		}
	}

	#[test]
	fn test_new_and_getters() {
		// Test successful creation and getters
		let traversal = Traversal::new(TraversalOrder::DepthFirst, 1, 8).unwrap();
		assert_eq!(traversal.order(), &TraversalOrder::DepthFirst);
		assert_eq!(traversal.get_max_size().unwrap(), 8);
	}

	#[test]
	fn test_new_any_size() {
		let traversal = Traversal::new_any_size(2, 4).unwrap();
		assert_eq!(traversal.order(), &TraversalOrder::AnyOrder);
		assert_eq!(traversal.get_max_size().unwrap(), 4);
	}

	#[test]
	fn test_new_any_and_default() {
		let any = Traversal::new_any();
		let def: Traversal = Default::default();
		assert_eq!(any, def);
		assert_eq!(any.order(), &TraversalOrder::AnyOrder);
		// default size covers full range
		assert_eq!(any.get_max_size().unwrap(), 1 << 20);
	}

	#[test]
	fn test_invalid_size_errors() {
		// zero or min > max should error
		assert!(Traversal::new(TraversalOrder::AnyOrder, 0, 1).is_err());
		assert!(Traversal::new(TraversalOrder::AnyOrder, 4, 2).is_err());
	}

	#[test]
	fn test_intersect_and_get_intersected() {
		let mut t1 = Traversal::new(TraversalOrder::AnyOrder, 1, 16).unwrap();
		let t2 = Traversal::new(TraversalOrder::DepthFirst, 2, 8).unwrap();
		// in-place intersect
		t1.intersect(&t2).unwrap();
		assert_eq!(t1.order(), &TraversalOrder::DepthFirst);
		assert_eq!(t1.get_max_size().unwrap(), 8);
		// get_intersected returns a new instance and does not modify original
		let t3 = Traversal::new(TraversalOrder::PMTiles, 4, 64).unwrap();
		let got = t3.get_intersected(&Traversal::new_any_size(2, 16).unwrap()).unwrap();
		assert_eq!(got.order(), &TraversalOrder::PMTiles);
		assert_eq!(got.get_max_size().unwrap(), 16);
	}
}
