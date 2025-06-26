mod image;
mod vector;

use crate::{traits::*, vpl::VPLNode, PipelineFactory};
use anyhow::{bail, Result};
use async_trait::async_trait;
use futures::future::BoxFuture;
use image::create_debug_image;
use std::fmt::Debug;
use vector::create_debug_vector_tile;
use versatiles_core::{tilejson::TileJSON, types::*};
use versatiles_image::helper::{image2blob, image2blob_fast};

#[derive(versatiles_derive::VPLDecode, Clone, Debug)]
/// Produces debugging tiles, each showing their coordinates as text.
struct Args {
	/// tile format: "mvt", "jpg", "png" or "webp"
	format: String,
	/// use fast compression
	fast: bool,
}

#[derive(Debug)]
pub struct Operation {
	tilejson: TileJSON,
	parameters: TilesReaderParameters,
	fast_compression: bool,
}

impl Operation {
	pub fn from_parameters(tile_format: TileFormat, fast_compression: bool) -> Result<Box<dyn OperationTrait>> {
		let parameters = TilesReaderParameters::new(
			tile_format,
			TileCompression::Uncompressed,
			TileBBoxPyramid::new_full(31),
		);

		let mut tilejson = TileJSON::default();

		if tile_format == TileFormat::MVT {
			tilejson.merge(&TileJSON::try_from(
				r#"{"vector_layers":[
					{"id":"background","minzoom":0,"maxzoom":30},
					{"id":"debug_x","minzoom":0,"maxzoom":30},
					{"id":"debug_y","minzoom":0,"maxzoom":30},
					{"id":"debug_z","minzoom":0,"maxzoom":30}
				]}"#,
			)?)?;
		}

		Ok(Box::new(Self {
			tilejson,
			parameters,
			fast_compression,
		}) as Box<dyn OperationTrait>)
	}
	pub fn from_vpl_node(vpl_node: &VPLNode) -> Result<Box<dyn OperationTrait>> {
		let args = Args::from_vpl_node(vpl_node)?;
		Self::from_parameters(TileFormat::parse_str(&args.format)?, args.fast)
	}
}

fn build_tile(coord: &TileCoord3, format: TileFormat, fast_compression: bool) -> Result<Option<Blob>> {
	Ok(Some(match format {
		TileFormat::JPG | TileFormat::PNG | TileFormat::WEBP => {
			let image = create_debug_image(coord);
			if fast_compression {
				image2blob_fast(&image, format)?
			} else {
				image2blob(&image, format)?
			}
		}
		TileFormat::MVT => create_debug_vector_tile(coord)?,
		_ => bail!("tile format '{format}' is not implemented yet"),
	}))
}

impl ReadOperationTrait for Operation {
	fn build(vpl_node: VPLNode, _factory: &PipelineFactory) -> BoxFuture<'_, Result<Box<dyn OperationTrait>>>
	where
		Self: Sized + OperationTrait,
	{
		Box::pin(async move { Operation::from_vpl_node(&vpl_node) })
	}
}

#[async_trait]
impl OperationTrait for Operation {
	fn get_parameters(&self) -> &TilesReaderParameters {
		&self.parameters
	}

	fn get_tilejson(&self) -> &TileJSON {
		&self.tilejson
	}

	async fn get_tile_data(&self, coord: &TileCoord3) -> Result<Option<Blob>> {
		build_tile(coord, self.parameters.tile_format, self.fast_compression)
	}

	async fn get_tile_stream(&self, bbox: TileBBox) -> TileStream {
		let format = self.parameters.tile_format;
		let fast = self.fast_compression;

		TileStream::from_coord_iter_parallel(bbox.into_iter_coords(), move |c| {
			build_tile(&c, format, fast).ok().flatten()
		})
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

	async fn test(format: &str, len: u64, meta: &str) -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let operation = factory
			.operation_from_vpl(&format!("from_debug format={format}"))
			.await?;

		let coord = TileCoord3 { x: 1, y: 2, z: 3 };
		let blob = operation.get_tile_data(&coord).await?.unwrap();

		assert_eq!(blob.len(), len, "for '{format}'");
		assert_eq!(operation.get_tilejson().as_string(), meta, "for '{format}'");

		let mut stream = operation.get_tile_stream(TileBBox::new(3, 1, 1, 2, 3)?).await;

		let mut n = 0;
		while let Some((coord, blob)) = stream.next().await {
			assert!(!blob.is_empty(), "for '{format}'");
			assert!(coord.x >= 1 && coord.x <= 2, "for '{format}'");
			assert!(coord.y >= 1 && coord.y <= 3, "for '{format}'");
			assert_eq!(coord.z, 3, "for '{format}'");
			n += 1;
		}
		assert_eq!(n, 6, "for '{format}'");

		Ok(())
	}

	#[tokio::test]
	async fn test_build_tile_png() {
		test("png", 5207, "{\"tilejson\":\"3.0.0\"}").await.unwrap();
	}

	#[tokio::test]
	async fn test_build_tile_jpg() {
		test("jpg", 11782, "{\"tilejson\":\"3.0.0\"}").await.unwrap();
	}

	#[tokio::test]
	async fn test_build_tile_webp() {
		test("webp", 2656, "{\"tilejson\":\"3.0.0\"}").await.unwrap();
	}

	#[tokio::test]
	async fn test_build_tile_vector() {
		test("mvt", 1732, "{\"tilejson\":\"3.0.0\",\"vector_layers\":[{\"fields\":{},\"id\":\"background\",\"maxzoom\":30,\"minzoom\":0},{\"fields\":{},\"id\":\"debug_x\",\"maxzoom\":30,\"minzoom\":0},{\"fields\":{},\"id\":\"debug_y\",\"maxzoom\":30,\"minzoom\":0},{\"fields\":{},\"id\":\"debug_z\",\"maxzoom\":30,\"minzoom\":0}]}").await.unwrap();
	}
}
