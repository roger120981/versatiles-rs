//! # Debug tile generator
//!
//! This operation produces *synthetic* raster or vector tiles whose only
//! purpose is to **visualise tile coordinates** during development—
//! extremely useful when verifying bounding‑box logic or inspecting
//! pyramids in a viewer.  
//!  
//! * For **raster** formats (`png`, `jpg`, `webp`, `avif`) each tile shows
//!   its *x*, *y*, *z* as white text on a coloured background.  
//! * For the **vector** format (`mvt`) the tile contains four simple layers
//!   (`background`, `debug_x`, `debug_y`, `debug_z`) whose geometries encode
//!   exactly the same coordinate information.
//!
//! Because the data are generated on‑the‑fly, no external storage is
//! required and the entire pyramid is always “complete.”

mod image;
mod vector;

use crate::{PipelineFactory, operations::read::traits::ReadOperationTrait, traits::*, vpl::VPLNode};
use anyhow::{Result, bail};
use async_trait::async_trait;
use image::create_debug_image;
use std::fmt::Debug;
use vector::create_debug_vector_tile;
use versatiles_container::Tile;
use versatiles_core::*;

#[derive(versatiles_derive::VPLDecode, Clone, Debug)]
/// Generates debug tiles that display their coordinates as text.
struct Args {
	/// Target tile format: one of `"mvt"` (default), `"avif"`, `"jpg"`, `"png"` or `"webp"`
	format: Option<String>,
}

/// Implements [`OperationTrait`] by fabricating debug tiles entirely in
/// memory.  No I/O other than the caller’s request/response is performed.
#[derive(Debug)]
pub struct Operation {
	tilejson: TileJSON,
	parameters: TilesReaderParameters,
}

impl Operation {
	pub fn from_parameters(tile_format: TileFormat) -> Result<Self> {
		let parameters = TilesReaderParameters::new(
			tile_format,
			TileCompression::Uncompressed,
			TileBBoxPyramid::new_full(30),
		);

		let mut tilejson = TileJSON::default();

		if tile_format.to_type() == TileType::Vector {
			tilejson.merge(&TileJSON::try_from(
				r#"{"vector_layers":[
					{"id":"background","minzoom":0,"maxzoom":30},
					{"id":"debug_x","minzoom":0,"maxzoom":30,"fields":{"char":"which character","index":"index of char","x":"position"}},
					{"id":"debug_y","minzoom":0,"maxzoom":30,"fields":{"char":"which character","index":"index of char","x":"position"}},
					{"id":"debug_z","minzoom":0,"maxzoom":30,"fields":{"char":"which character","index":"index of char","x":"position"}}
				]}"#,
			)?)?;
		}

		tilejson.update_from_reader_parameters(&parameters);

		Ok(Self { tilejson, parameters })
	}
	pub fn from_vpl_node(vpl_node: &VPLNode) -> Result<Self> {
		let args = Args::from_vpl_node(vpl_node)?;
		Self::from_parameters(
			args
				.format
				.map(|f| TileFormat::parse_str(&f))
				.transpose()?
				.unwrap_or(TileFormat::MVT),
		)
	}
}

impl ReadOperationTrait for Operation {
	async fn build(vpl_node: VPLNode, _factory: &PipelineFactory) -> Result<Box<dyn OperationTrait>>
	where
		Self: Sized + OperationTrait,
	{
		Operation::from_vpl_node(&vpl_node).map(|op| Box::new(op) as Box<dyn OperationTrait>)
	}
}

#[async_trait]
impl OperationTrait for Operation {
	/// Return static reader parameters (compression *always* uncompressed).
	fn parameters(&self) -> &TilesReaderParameters {
		&self.parameters
	}

	/// Return a synthetic `TileJSON` that matches the chosen debug format.
	fn tilejson(&self) -> &TileJSON {
		&self.tilejson
	}

	async fn get_stream(&self, bbox: TileBBox) -> Result<TileStream<Tile>> {
		log::debug!("get_stream {:?}", bbox);
		let format = self.parameters.tile_format;
		match self.parameters.tile_format.to_type() {
			TileType::Raster => {
				let alpha = format != TileFormat::JPG;
				Ok(TileStream::from_iter_coord_parallel(
					bbox.into_iter_coords(),
					move |c| Some(Tile::from_image(create_debug_image(&c, alpha), format).unwrap()),
				))
			}
			TileType::Vector => Ok(TileStream::from_iter_coord_parallel(
				bbox.into_iter_coords(),
				move |c| Some(Tile::from_vector(create_debug_vector_tile(&c).unwrap(), format).unwrap()),
			)),
			_ => bail!("tile format '{}' is not supported.", self.parameters.tile_format),
		}
	}
}

pub struct Factory {}

impl OperationFactoryTrait for Factory {
	fn get_docs(&self) -> String {
		Args::get_docs()
	}
	fn get_tag_name(&self) -> &str {
		"from_debug"
	}
}

#[async_trait]
impl ReadOperationFactoryTrait for Factory {
	async fn build<'a>(&self, vpl_node: VPLNode, factory: &'a PipelineFactory) -> Result<Box<dyn OperationTrait>> {
		Operation::build(vpl_node, factory).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use versatiles_core::TileCompression::Uncompressed;

	async fn test(format: &str, len: u64, tilejson: &[&str]) -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let operation = factory
			.operation_from_vpl(&format!("from_debug format={format}"))
			.await?;

		let coord = TileCoord { x: 1, y: 2, level: 3 };
		let tile = operation
			.get_stream(coord.as_tile_bbox(1)?)
			.await?
			.next()
			.await
			.unwrap()
			.1;

		assert_eq!(tile.into_blob(Uncompressed)?.len(), len, "for '{format}'");
		assert_eq!(operation.tilejson().as_pretty_lines(100), tilejson, "for '{format}'");

		let mut stream = operation.get_stream(TileBBox::from_min_and_max(3, 1, 1, 2, 3)?).await?;

		let mut n = 0;
		while let Some((coord, tile)) = stream.next().await {
			assert!(!tile.into_blob(Uncompressed)?.is_empty(), "for '{format}'");
			assert!(coord.x >= 1 && coord.x <= 2, "for '{format}'");
			assert!(coord.y >= 1 && coord.y <= 3, "for '{format}'");
			assert_eq!(coord.level, 3, "for '{format}'");
			n += 1;
		}
		assert_eq!(n, 6, "for '{format}'");

		Ok(())
	}

	#[tokio::test]
	async fn test_build_tile_avif() {
		test(
			"avif",
			8528,
			&[
				"{",
				"  \"bounds\": [-180, -85.051129, 180, 85.051129],",
				"  \"maxzoom\": 30,",
				"  \"minzoom\": 0,",
				"  \"tile_format\": \"image/avif\",",
				"  \"tile_schema\": \"rgb\",",
				"  \"tile_type\": \"raster\",",
				"  \"tilejson\": \"3.0.0\"",
				"}",
			],
		)
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn test_build_tile_jpg() {
		test(
			"jpg",
			11862,
			&[
				"{",
				"  \"bounds\": [-180, -85.051129, 180, 85.051129],",
				"  \"maxzoom\": 30,",
				"  \"minzoom\": 0,",
				"  \"tile_format\": \"image/jpeg\",",
				"  \"tile_schema\": \"rgb\",",
				"  \"tile_type\": \"raster\",",
				"  \"tilejson\": \"3.0.0\"",
				"}",
			],
		)
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn test_build_tile_png() {
		test(
			"png",
			6388,
			&[
				"{",
				"  \"bounds\": [-180, -85.051129, 180, 85.051129],",
				"  \"maxzoom\": 30,",
				"  \"minzoom\": 0,",
				"  \"tile_format\": \"image/png\",",
				"  \"tile_schema\": \"rgb\",",
				"  \"tile_type\": \"raster\",",
				"  \"tilejson\": \"3.0.0\"",
				"}",
			],
		)
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn test_build_tile_webp() {
		test(
			"webp",
			3756,
			&[
				"{",
				"  \"bounds\": [-180, -85.051129, 180, 85.051129],",
				"  \"maxzoom\": 30,",
				"  \"minzoom\": 0,",
				"  \"tile_format\": \"image/webp\",",
				"  \"tile_schema\": \"rgb\",",
				"  \"tile_type\": \"raster\",",
				"  \"tilejson\": \"3.0.0\"",
				"}",
			],
		)
		.await
		.unwrap();
	}

	#[tokio::test]
	async fn test_build_tile_vector() {
		test(
			"mvt",
			1996,
			&[
				"{",
				"  \"bounds\": [-180, -85.051129, 180, 85.051129],",
				"  \"maxzoom\": 30,",
				"  \"minzoom\": 0,",
				"  \"tile_format\": \"vnd.mapbox-vector-tile\",",
				"  \"tile_schema\": \"other\",",
				"  \"tile_type\": \"vector\",",
				"  \"tilejson\": \"3.0.0\",",
				"  \"vector_layers\": [",
				"    { \"fields\": {  }, \"id\": \"background\", \"maxzoom\": 30, \"minzoom\": 0 },",
				"    {",
				"      \"fields\": { \"char\": \"which character\", \"index\": \"index of char\", \"x\": \"position\" },",
				"      \"id\": \"debug_x\",",
				"      \"maxzoom\": 30,",
				"      \"minzoom\": 0",
				"    },",
				"    {",
				"      \"fields\": { \"char\": \"which character\", \"index\": \"index of char\", \"x\": \"position\" },",
				"      \"id\": \"debug_y\",",
				"      \"maxzoom\": 30,",
				"      \"minzoom\": 0",
				"    },",
				"    {",
				"      \"fields\": { \"char\": \"which character\", \"index\": \"index of char\", \"x\": \"position\" },",
				"      \"id\": \"debug_z\",",
				"      \"maxzoom\": 30,",
				"      \"minzoom\": 0",
				"    }",
				"  ]",
				"}",
			],
		)
		.await
		.unwrap();
	}
}
