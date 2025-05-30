//! `*.versatiles` container
//!
//! see [specification](https://github.com/versatiles-org/versatiles-spec)
//!
//! This module provides functionality to read from and write to `*.versatiles` container files.
//!
//! # Usage Example
//!
//! ```rust
//! use versatiles_container::{MBTilesReader, TilesWriterTrait, VersaTilesReader, VersaTilesWriter};
//! use versatiles_core::types::{TileCoord3, TilesReaderTrait};
//! use std::path::Path;
//! use anyhow::Result;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let path_mbtiles = std::env::current_dir()?.join("../testdata/berlin.mbtiles");
//!     let path_versatiles = std::env::current_dir()?.join("../testdata/temp4.versatiles");
//!
//!     // Create a mbtiles reader
//!     let mut reader = MBTilesReader::open_path(&path_mbtiles)?;
//!
//!     // Write the tiles to the .versatiles file
//!     VersaTilesWriter::write_to_path(&mut reader, &path_versatiles).await?;
//!
//!     println!("Tiles have been successfully written to {path_versatiles:?}");
//!
//!     // Read the tiles back from the .versatiles file
//!     let mut reader = VersaTilesReader::open_path(&path_versatiles).await?;
//!
//!     // Get tile data
//!     if let Some(tile) = reader.get_tile_data(&TileCoord3::new(2200, 1345, 12)?).await? {
//!         println!("Tile data: {tile:?}");
//!     } else {
//!         println!("No tile data found");
//!     }
//!
//!     Ok(())
//! }
//! ```

mod types;

mod reader;
pub use reader::VersaTilesReader;

mod writer;
pub use writer::VersaTilesWriter;
