//! Provides functionality for reading tile data from a PMTiles container.
//!
//! The `PMTilesReader` struct is the primary component of this module, offering methods to read metadata and tile data from a PMTiles container.
//!
//! ## Features
//! - Supports reading metadata and tile data with internal compression
//! - Provides methods to query the container for tile data based on coordinates
//! - Implements caching for efficient data retrieval
//!
//! ## Usage Example
//! ```rust
//! use versatiles_container::PMTilesReader;
//! use versatiles_core::types::{TileCoord3, TilesReaderTrait};
//! use std::path::Path;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Open the PMTiles container
//!     let path = std::env::current_dir()?.join("../testdata/berlin.pmtiles");
//!     let mut reader = PMTilesReader::open_path(&path).await?;
//!
//!     // Get metadata
//!     println!("Metadata: {:?}", reader.get_tilejson());
//!
//!     // Get tile data for specific coordinates
//!     let coord = TileCoord3::new(1, 1, 1)?;
//!     if let Some(tile_data) = reader.get_tile_data(&coord).await? {
//!         println!("Tile data: {:?}", tile_data);
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! ## Errors
//! - Returns errors if the container file does not exist, if the path is not absolute, or if there are issues querying the container.
//!
//! ## Testing
//! This module includes comprehensive tests to ensure the correct functionality of reading metadata, handling different file formats, and verifying tile data.

use super::types::{tile_id_to_coord, EntriesV3, HeaderV3, TileId};
use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::lock::Mutex;
use std::{fmt::Debug, path::Path, sync::Arc};
#[cfg(feature = "cli")]
use versatiles_core::utils::PrettyPrint;
use versatiles_core::{io::*, tilejson::TileJSON, types::*, utils::decompress};

/// A struct that provides functionality to read tile data from a PMTiles container.
#[derive(Debug)]
pub struct PMTilesReader {
	pub data_reader: DataReader,
	pub header: HeaderV3,
	pub internal_compression: TileCompression,
	pub leaves_bytes: Blob,
	pub leaves_cache: Mutex<LimitedCache<ByteRange, Arc<Blob>>>,
	pub tilejson: TileJSON,
	pub parameters: TilesReaderParameters,
	pub root_bytes_uncompressed: Arc<Blob>,
}

impl PMTilesReader {
	/// Creates a new `PMTilesReader` from a given filename.
	///
	/// # Arguments
	/// * `path` - The path to the PMTiles container file.
	///
	/// # Errors
	/// Returns an error if the file does not exist or if there is an error opening the reader.
	pub async fn open_path(path: &Path) -> Result<PMTilesReader> {
		PMTilesReader::open_reader(DataReaderFile::open(path)?).await
	}

	/// Creates a new `PMTilesReader` from a given `DataReader`.
	///
	/// # Arguments
	/// * `data_reader` - A data reader for the PMTiles container.
	///
	/// # Errors
	/// Returns an error if there is an issue reading or decompressing data.
	pub async fn open_reader(data_reader: DataReader) -> Result<PMTilesReader>
	where
		Self: Sized,
	{
		let header = HeaderV3::deserialize(&data_reader.read_range(&ByteRange::new(0, HeaderV3::len())).await?)?;

		let internal_compression = header.internal_compression.as_value()?;

		let meta = data_reader.read_range(&header.metadata).await?;
		let meta = decompress(meta, &internal_compression)?;
		let tilejson = TileJSON::try_from_blob_or_default(&meta);

		let root_bytes_uncompressed = decompress(data_reader.read_range(&header.root_dir).await?, &internal_compression)?;
		let leaves_bytes = data_reader.read_range(&header.leaf_dirs).await?;

		let bbox_pyramid = calc_bbox_pyramid(&root_bytes_uncompressed, &leaves_bytes, &internal_compression)?;

		let parameters = TilesReaderParameters::new(
			header.tile_type.as_value()?,
			header.tile_compression.as_value()?,
			bbox_pyramid,
		);

		Ok(PMTilesReader {
			data_reader,
			header,
			internal_compression,
			leaves_bytes,
			leaves_cache: Mutex::new(LimitedCache::with_maximum_size(100_000_000)),
			tilejson,
			parameters,
			root_bytes_uncompressed: Arc::new(root_bytes_uncompressed),
		})
	}

	pub fn get_tile_entries(&self) -> Result<EntriesV3> {
		EntriesV3::from_blob(&self.root_bytes_uncompressed)
	}
}

/// Calculates the bounding box pyramid from the provided data.
fn calc_bbox_pyramid(
	root_bytes_uncompressed: &Blob,
	leaves_bytes: &Blob,
	compression: &TileCompression,
) -> Result<TileBBoxPyramid> {
	let mut bbox_pyramid = TileBBoxPyramid::new_empty();

	parse_directories(&mut bbox_pyramid, root_bytes_uncompressed, leaves_bytes, compression)?;

	fn parse_directories(
		bbox_pyramid: &mut TileBBoxPyramid,
		dir: &Blob,
		leaves_bytes: &Blob,
		compression: &TileCompression,
	) -> Result<()> {
		let entries = EntriesV3::from_blob(dir)?;
		for entry in entries.iter() {
			if entry.range.length > 0 {
				if entry.run_length > 0 {
					for i in 0..entry.run_length as u64 {
						let coord = tile_id_to_coord(i + entry.tile_id)?;
						bbox_pyramid.include_coord(&coord);
					}
				} else {
					let range = entry.range;
					let mut blob = leaves_bytes.read_range(&range)?;
					blob = decompress(blob, compression)?;
					parse_directories(bbox_pyramid, &blob, leaves_bytes, compression)?;
				}
			}
		}
		Ok(())
	}

	Ok(bbox_pyramid)
}

#[async_trait]
impl TilesReaderTrait for PMTilesReader {
	/// Returns the container name.
	fn get_container_name(&self) -> &str {
		"pmtiles"
	}

	/// Returns the parameters of the tiles reader.
	fn get_parameters(&self) -> &TilesReaderParameters {
		&self.parameters
	}

	/// Overrides the tile compression method.
	///
	/// # Arguments
	/// * `tile_compression` - The new tile compression method.
	fn override_compression(&mut self, tile_compression: TileCompression) {
		self.parameters.tile_compression = tile_compression;
	}

	/// Returns the metadata as a `Blob`.
	///
	/// # Errors
	/// Returns an error if there is an issue retrieving the metadata.
	fn get_tilejson(&self) -> &TileJSON {
		&self.tilejson
	}

	/// Returns the name of the PMTiles container.
	fn get_source_name(&self) -> &str {
		self.data_reader.get_name()
	}

	/// Returns the tile data for the specified coordinates as a `Blob`.
	///
	/// # Arguments
	/// * `coord` - The coordinates of the tile.
	///
	/// # Errors
	/// Returns an error if there is an issue retrieving the tile data.
	async fn get_tile_data(&self, coord: &TileCoord3) -> Result<Option<Blob>> {
		// Log the requested tile coordinates for debugging purposes
		log::trace!("get_tile_data {:?}", coord);

		// Convert the tile coordinates into a unique tile ID
		let tile_id: u64 = coord.get_tile_id()?;
		// Start with the root directory bytes
		let mut dir_bytes = self.root_bytes_uncompressed.clone();

		// Iterate through the directory depth (up to 3 levels)
		for _depth in 0..3 {
			// Deserialize the directory entries from the current directory bytes
			let entries = EntriesV3::from_blob(&dir_bytes)?;
			// Find the entry corresponding to the requested tile ID
			let entry = entries.find_tile(tile_id);

			// If the entry is not found, return None
			let entry = if let Some(entry) = entry {
				entry
			} else {
				return Ok(None);
			};

			// Check if the entry has a valid range
			if entry.range.length > 0 {
				// If the entry represents a run of tiles, directly fetch the tile data
				if entry.run_length > 0 {
					return Ok(Some(
						self
							.data_reader
							.read_range(&entry.range.get_shifted_forward(self.header.tile_data.offset))
							.await?,
					));
				} else {
					// Otherwise, fetch the directory bytes for the next level
					let range = entry.range;
					let mut cache = self.leaves_cache.lock().await;
					// Use the cache to avoid redundant decompression and reading
					dir_bytes = cache.get_or_set(&range, || {
						let mut blob = self.leaves_bytes.read_range(&range)?;
						// Decompress the directory bytes
						blob = decompress(blob, &self.internal_compression)?;
						Ok(Arc::new(blob))
					})?;
				}
			} else {
				// If the range is invalid, return None
				return Ok(None);
			}
		}

		// If the tile data is not found after traversing all levels, return an error
		bail!("not found")
	}

	// deep probe of container meta
	#[cfg(feature = "cli")]
	async fn probe_container(&mut self, print: &PrettyPrint) -> Result<()> {
		print.add_key_value("header", &self.header).await;

		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use lazy_static::lazy_static;
	use std::{env::current_dir, path::PathBuf};
	use versatiles_core::assert_wildcard;

	lazy_static! {
		static ref PATH: PathBuf = current_dir().unwrap().join("../testdata/berlin.pmtiles");
	}

	#[tokio::test]
	async fn reader() -> Result<()> {
		let reader = PMTilesReader::open_path(&PATH).await?;

		assert_eq!(reader.get_container_name(), "pmtiles");

		assert_wildcard!(reader.get_source_name(), "*testdata?berlin.pmtiles");

		assert_eq!(format!("{:?}", reader.header), "HeaderV3 { root_dir: ByteRange[127,2271], metadata: ByteRange[2398,592], leaf_dirs: ByteRange[2990,0], tile_data: ByteRange[2990,25869006], addressed_tiles_count: 878, tile_entries_count: 878, tile_contents_count: 876, clustered: true, internal_compression: Gzip, tile_compression: Gzip, tile_type: MVT, min_zoom: 0, max_zoom: 14, min_lon_e7: 130828300, min_lat_e7: 523344600, max_lon_e7: 137622450, max_lat_e7: 526783000, center_zoom: 7, center_lon_e7: 134225380, center_lat_e7: 525063800 }");

		assert_wildcard!(
			reader.get_tilejson().as_string(),
			"{\"author\":\"OpenStreetMap contributors, Geofabrik GmbH\",*,\"version\":\"3.0\"}"
		);

		assert_wildcard!(
			format!("{:?}", reader.get_parameters()), 
			"TilesReaderParameters { bbox_pyramid: [0: [0,0,0,0] (1), 1: [1,0,1,0] (1), 2: [2,1,2,1] (1), 3: [4,2,4,2] (1), 4: [8,5,8,5] (1), 5: [17,10,17,10] (1), 6: [34,20,34,21] (2), 7: [68,41,68,42] (2), 8: [137,83,137,84] (2), 9: [274,167,275,168] (4), 10: [549,335,551,336] (6), 11: [1098,670,1102,673] (20), 12: [2196,1340,2204,1346] (63), 13: [4393,2680,4409,2693] (238), 14: [8787,5361,8818,5387] (864)], tile_compression: Gzip, tile_format: MVT }"
		);

		assert_eq!(
			reader.get_tile_data(&TileCoord3::new(0, 0, 0)?).await?.unwrap().len(),
			20
		);

		assert_eq!(
			reader
				.get_tile_data(&TileCoord3::new(8800, 5370, 14)?)
				.await?
				.unwrap()
				.len(),
			100391
		);

		assert!(reader.get_tile_data(&TileCoord3::new(0, 0, 16)?).await?.is_none());

		Ok(())
	}
}
