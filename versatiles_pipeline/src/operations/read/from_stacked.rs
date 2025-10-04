//! # from_stacked operation
//!
//! Selects the **first** non‑empty tile from a chain of sources that all
//! share the *same* tile type (raster *or* vector).  Think of it as a
//! “transparent overlay”: the moment a source can deliver a tile for the
//! requested coordinate, downstream sources are ignored for that tile.
//!
//! * Sources are evaluated in the **order** provided in the VPL list.  
//! * No blending occurs – it is a *winner‑takes‑first* strategy.  
//! * All sources must expose an identical tile type and compression; only
//!   their spatial coverage may differ.
//!
//! The file provides:
//! 1. [`Args`] – CLI / VPL configuration,  
//! 2. [`Operation`] – the runtime implementation,  
//! 3. Tests that verify error handling and overlay semantics.

use crate::{
	PipelineFactory,
	helpers::Tile,
	operations::read::traits::ReadOperationTrait,
	traits::*,
	vpl::{VPLNode, VPLPipeline},
};
use anyhow::{Result, ensure};
use async_trait::async_trait;
use futures::{
	StreamExt,
	future::{BoxFuture, join_all},
	stream,
};
use versatiles_core::{tilejson::TileJSON, *};

#[derive(versatiles_derive::VPLDecode, Clone, Debug)]
/// Overlays multiple tile sources, using the tile from the first source that provides it.
struct Args {
	/// All tile sources must have the same format.
	sources: Vec<VPLPipeline>,
}

#[derive(Debug)]
/// Implements [`OperationTrait`] by performing *short‑circuit* look‑ups
/// across multiple sources.
///
/// The struct keeps only metadata (`TileJSON`, `TilesReaderParameters`) in
/// memory; actual tile data are streamed directly from the first source that
/// contains them.
struct Operation {
	parameters: TilesReaderParameters,
	sources: Vec<Box<dyn OperationTrait>>,
	tilejson: TileJSON,
	traversal: Traversal,
}

impl ReadOperationTrait for Operation {
	fn build(
		vpl_node: VPLNode,
		factory: &PipelineFactory,
	) -> BoxFuture<'_, Result<Box<dyn OperationTrait>, anyhow::Error>>
	where
		Self: Sized + OperationTrait,
	{
		Box::pin(async move {
			let args = Args::from_vpl_node(&vpl_node)?;
			let sources = join_all(args.sources.into_iter().map(|c| factory.build_pipeline(c)))
				.await
				.into_iter()
				.collect::<Result<Vec<_>>>()?;

			Ok(Box::new(Operation::new(sources)?) as Box<dyn OperationTrait>)
		})
	}
}

impl Operation {
	fn new(sources: Vec<Box<dyn OperationTrait>>) -> Result<Operation> {
		ensure!(sources.len() > 1, "must have at least two sources");

		let mut tilejson = TileJSON::default();
		let parameters = sources.first().unwrap().parameters();
		let tile_format = parameters.tile_format;
		let tile_compression = parameters.tile_compression;

		let mut pyramid = TileBBoxPyramid::new_empty();
		let mut traversal = Traversal::default();

		for source in sources.iter() {
			tilejson.merge(source.tilejson())?;

			traversal.intersect(source.traversal())?;

			let parameters = source.parameters();
			pyramid.include_bbox_pyramid(&parameters.bbox_pyramid);

			ensure!(
				parameters.tile_format == tile_format,
				"all sources must have the same tile format"
			);
		}

		let parameters = TilesReaderParameters::new(tile_format, tile_compression, pyramid);
		tilejson.update_from_reader_parameters(&parameters);

		Ok(Self {
			tilejson,
			parameters,
			sources,
			traversal,
		})
	}
}

#[async_trait]
impl OperationTrait for Operation {
	/// Reader parameters (format, compression, pyramid) for the overlay result.
	fn parameters(&self) -> &TilesReaderParameters {
		&self.parameters
	}

	/// Combined `TileJSON` after merging metadata from all sources.
	fn tilejson(&self) -> &TileJSON {
		&self.tilejson
	}

	fn traversal(&self) -> &Traversal {
		&self.traversal
	}

	/// Stream packed tiles intersecting `bbox` using the overlay strategy.
	async fn get_stream(&self, bbox: TileBBox) -> Result<TileStream<Tile>> {
		log::debug!("get_stream {:?}", bbox);
		// We need the desired output compression inside the closure, so copy it.
		let format = self.parameters.tile_format;
		let compression = self.parameters.tile_compression;

		let sub_bboxes: Vec<TileBBox> = bbox.clone().iter_bbox_grid(32).collect();

		Ok(TileStream::from_streams(stream::iter(sub_bboxes).map(
			move |bbox| async move {
				let mut tiles = TileBBoxMap::<Option<Tile>>::new_default(bbox);

				for source in self.sources.iter() {
					let mut bbox_left = TileBBox::new_empty(bbox.level).unwrap();
					for (coord, slot) in tiles.iter() {
						if slot.is_none() {
							bbox_left.include_coord(&coord).unwrap();
						}
					}
					if bbox_left.is_empty() {
						continue;
					}

					let stream = source.get_stream(bbox_left).await.unwrap();
					stream
						.for_each_sync(|(coord, mut tile)| {
							let entry = tiles.get_mut(&coord).unwrap();
							if entry.is_none() {
								tile.change_format(format).unwrap();
								tile.change_compression(compression).unwrap();
								*entry = Some(tile);
							}
						})
						.await;
				}
				let vec = tiles
					.into_iter()
					.flat_map(|(coord, item)| item.map(|tile| (coord, tile)))
					.collect::<Vec<_>>();
				TileStream::from_vec(vec)
			},
		)))
	}
}

pub struct Factory {}

impl OperationFactoryTrait for Factory {
	fn get_docs(&self) -> String {
		Args::get_docs()
	}
	fn get_tag_name(&self) -> &str {
		"from_stacked"
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
	use crate::helpers::{arrange_tiles, dummy_vector_source::DummyVectorSource};
	use std::sync::LazyLock;
	use versatiles_core::TraversalOrder;

	static RESULT_PATTERN: LazyLock<Vec<String>> = LazyLock::new(|| {
		vec![
			"🟦 🟦 🟦 🟦 ❌ ❌".to_string(),
			"🟦 🟦 🟦 🟦 ❌ ❌".to_string(),
			"🟦 🟦 🟦 🟦 🟨 🟨".to_string(),
			"🟦 🟦 🟦 🟦 🟨 🟨".to_string(),
			"❌ ❌ 🟨 🟨 🟨 🟨".to_string(),
			"❌ ❌ 🟨 🟨 🟨 🟨".to_string(),
		]
	});

	pub fn check_vector(tile: Tile) -> String {
		let tile = tile.into_vector().unwrap();
		assert_eq!(tile.layers.len(), 1);

		let layer = &tile.layers[0];
		assert_eq!(layer.name, "dummy");
		assert_eq!(layer.features.len(), 1);

		let feature = &layer.features[0].to_feature(layer).unwrap();
		let properties = &feature.properties;

		let filename = properties.get("filename").unwrap().to_string();
		filename[0..filename.len() - 4].to_string()
	}

	pub fn check_image(tile: Tile) -> String {
		use versatiles_image::traits::*;
		let image = tile.into_image().unwrap();
		let pixel = image.average_color();
		match pixel.as_slice() {
			[0, 0, 255] => "🟦".to_string(),
			[255, 255, 0] => "🟨".to_string(),
			_ => panic!("Unexpected pixel color: {pixel:?}"),
		}
	}

	#[tokio::test]
	async fn test_operation_error() {
		let factory = PipelineFactory::new_dummy();
		let error = |command: &'static str| async {
			assert_eq!(
				factory.operation_from_vpl(command).await.unwrap_err().to_string(),
				"must have at least two sources"
			)
		};

		error("from_stacked").await;
		error("from_stacked [ ]").await;
		error("from_stacked [ from_container filename=1.pbf ]").await;
	}

	#[tokio::test]
	async fn test_tilejson() -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let result = factory
			.operation_from_vpl(
				&[
					"from_stacked [",
					"   from_container filename=\"1.pbf\" | filter bbox=[-11,-12,3,4],",
					"   from_container filename=\"2.pbf\" | filter bbox=[-5,-6,7,8]",
					"]",
				]
				.join(""),
			)
			.await?;

		assert_eq!(
			result.tilejson().as_pretty_lines(100),
			[
				"{",
				"  \"bounds\": [ -11.25, -12.554564, 7.03125, 8.407168 ],",
				"  \"maxzoom\": 8,",
				"  \"minzoom\": 0,",
				"  \"name\": \"dummy vector source\",",
				"  \"tile_format\": \"vnd.mapbox-vector-tile\",",
				"  \"tile_schema\": \"other\",",
				"  \"tile_type\": \"vector\",",
				"  \"tilejson\": \"3.0.0\"",
				"}"
			]
		);

		Ok(())
	}

	#[tokio::test]
	async fn test_operation_vector() -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let result = factory
			.operation_from_vpl(
				&[
					"from_stacked [",
					"   from_container filename=\"🟦.pbf\" | filter bbox=[-130,-20,20,70],",
					"   from_container filename=\"🟨.pbf\" | filter bbox=[-20,-70,130,20]",
					"]",
				]
				.join(""),
			)
			.await?;

		let bbox = TileBBox::new_full(3)?;

		let tiles = result.get_stream(bbox).await?.to_vec().await;
		assert_eq!(arrange_tiles(tiles, check_vector), *RESULT_PATTERN);

		Ok(())
	}

	#[tokio::test]
	async fn test_operation_image() -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let result = factory
			.operation_from_vpl(
				&[
					"from_stacked [",
					"   from_container filename=\"00f.png\" | filter bbox=[-130,-20,20,70],",
					"   from_container filename=\"ff0.png\" | filter bbox=[-20,-70,130,20]",
					"]",
				]
				.join(""),
			)
			.await?;

		let bbox = TileBBox::new_full(3)?;

		let tiles = result.get_stream(bbox).await?.to_vec().await;
		assert_eq!(arrange_tiles(tiles, check_image), *RESULT_PATTERN);

		Ok(())
	}

	#[test]
	fn test_traversal_orders_overlay() {
		use crate::operations::read::from_container::operation_from_reader;

		let mut src1 = DummyVectorSource::new(&[], Some(TileBBoxPyramid::new_full(8)));
		let mut src2 = DummyVectorSource::new(&[], Some(TileBBoxPyramid::new_full(8)));

		src1.set_traversal(Traversal::new_any_size(1, 16).unwrap());
		src2.set_traversal(Traversal::new(TraversalOrder::PMTiles, 4, 256).unwrap());

		let op = Operation::new(vec![
			operation_from_reader(Box::new(src1)),
			operation_from_reader(Box::new(src2)),
		])
		.unwrap();

		assert_eq!(op.traversal(), &Traversal::new(TraversalOrder::PMTiles, 4, 16).unwrap());
	}
}
