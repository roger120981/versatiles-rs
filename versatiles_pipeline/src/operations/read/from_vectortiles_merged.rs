use crate::{
	helpers::{pack_vector_tile, pack_vector_tile_stream, unpack_vector_tile},
	operations::read::traits::ReadOperationTrait,
	traits::*,
	vpl::{VPLNode, VPLPipeline},
	PipelineFactory,
};
use anyhow::{bail, ensure, Result};
use async_trait::async_trait;
use futures::future::{join_all, BoxFuture};
use imageproc::image::DynamicImage;
use std::collections::HashMap;
use versatiles_core::{tilejson::TileJSON, types::*, utils::decompress};
use versatiles_geometry::vector_tile::{VectorTile, VectorTileLayer};

#[derive(versatiles_derive::VPLDecode, Clone, Debug)]
/// Merges multiple vector tile sources. Each layer will contain all features from the same layer of all sources.
struct Args {
	/// All tile sources must provide vector tiles.
	sources: Vec<VPLPipeline>,
}

#[derive(Debug)]
struct Operation {
	parameters: TilesReaderParameters,
	sources: Vec<Box<dyn OperationTrait>>,
	tilejson: TileJSON,
}

fn merge_vector_tiles(tiles: Vec<VectorTile>) -> Result<VectorTile> {
	let mut layers = HashMap::<String, VectorTileLayer>::new();
	for tile in tiles.into_iter() {
		for new_layer in tile.layers {
			if let Some(layer) = layers.get_mut(&new_layer.name) {
				layer.add_from_layer(new_layer)?;
			} else {
				layers.insert(new_layer.name.clone(), new_layer);
			}
		}
	}
	Ok(VectorTile::new(layers.into_values().collect()))
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

			ensure!(sources.len() > 1, "must have at least two sources");

			let mut meta = TileJSON::default();
			let parameters = sources.first().unwrap().get_parameters();
			let mut pyramid = parameters.bbox_pyramid.clone();
			let tile_format = parameters.tile_format;
			let tile_compression = TileCompression::Uncompressed;

			for source in sources.iter() {
				meta.merge(source.get_tilejson())?;

				let parameters = source.get_parameters();
				pyramid.include_bbox_pyramid(&parameters.bbox_pyramid);
				ensure!(tile_format == TileFormat::MVT, "all sources must be vector tiles");
			}

			let parameters = TilesReaderParameters::new(tile_format, tile_compression, pyramid);

			Ok(Box::new(Self {
				tilejson: meta,
				parameters,
				sources,
			}) as Box<dyn OperationTrait>)
		})
	}
}

impl OperationTrait for Operation {}

#[async_trait]
impl OperationBasicsTrait for Operation {
	fn get_parameters(&self) -> &TilesReaderParameters {
		&self.parameters
	}

	fn get_tilejson(&self) -> &TileJSON {
		&self.tilejson
	}
}

#[async_trait]
impl OperationTilesTrait for Operation {
	async fn get_tile_data(&self, coord: &TileCoord3) -> Result<Option<Blob>> {
		pack_vector_tile(
			self.get_vector_data(coord).await,
			self.parameters.tile_format,
			self.parameters.tile_compression,
		)
	}

	async fn get_tile_stream(&self, bbox: TileBBox) -> Result<TileStream> {
		pack_vector_tile_stream(
			self.get_vector_stream(bbox).await,
			self.parameters.tile_format,
			self.parameters.tile_compression,
		)
	}

	async fn get_image_data(&self, _coord: &TileCoord3) -> Result<Option<DynamicImage>> {
		bail!("this operation does not support image data");
	}

	async fn get_image_stream(&self, _bbox: TileBBox) -> Result<TileStream<DynamicImage>> {
		bail!("this operation does not support image data");
	}

	async fn get_vector_data(&self, coord: &TileCoord3) -> Result<Option<VectorTile>> {
		let mut vector_tiles: Vec<VectorTile> = vec![];
		for source in self.sources.iter() {
			let vector_tile = unpack_vector_tile(
				source.get_tile_data(coord).await,
				source.get_parameters().tile_format,
				source.get_parameters().tile_compression,
			)?;
			if let Some(vector_tile) = vector_tile {
				vector_tiles.push(vector_tile);
			}
		}

		Ok(if vector_tiles.is_empty() {
			None
		} else {
			Some(merge_vector_tiles(vector_tiles)?)
		})
	}

	async fn get_vector_stream(&self, bbox: TileBBox) -> Result<TileStream<VectorTile>> {
		let bboxes: Vec<TileBBox> = bbox.clone().iter_bbox_grid(32).collect();

		Ok(
			TileStream::from_stream_iter(bboxes.into_iter().map(move |bbox| async move {
				let mut tiles: Vec<Vec<VectorTile>> = Vec::new();
				tiles.resize(bbox.count_tiles() as usize, vec![]);

				for source in self.sources.iter() {
					source
						.get_tile_stream(bbox)
						.await
						.unwrap()
						.for_each_sync(|(coord, mut blob)| {
							let index = bbox.get_tile_index3(&coord).unwrap();
							blob = decompress(blob, &source.get_parameters().tile_compression).unwrap();
							tiles[index].push(VectorTile::from_blob(&blob).unwrap());
						})
						.await;
				}

				TileStream::from_vec(
					tiles
						.into_iter()
						.enumerate()
						.filter_map(|(i, v)| {
							if v.is_empty() {
								None
							} else {
								Some((
									bbox.get_coord3_by_index(i as u32).unwrap(),
									merge_vector_tiles(v).unwrap(),
								))
							}
						})
						.collect(),
				)
			}))
			.await,
		)
	}
}

pub struct Factory {}

impl OperationFactoryTrait for Factory {
	fn get_docs(&self) -> String {
		Args::get_docs()
	}
	fn get_tag_name(&self) -> &str {
		"from_vectortiles_merged"
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
	use crate::helpers::mock_vector_source::{arrange_tiles, MockVectorSource};
	use itertools::Itertools;
	use std::{ops::BitXor, path::Path};

	pub fn check_tile(blob: &Blob, coord: &TileCoord3) -> String {
		use versatiles_geometry::GeoValue;

		let tile = VectorTile::from_blob(blob).unwrap();
		assert_eq!(tile.layers.len(), 1);

		let layer = &tile.layers[0];
		assert_eq!(layer.name, "mock");

		layer
			.features
			.iter()
			.map(|vtf| {
				let p = vtf.to_feature(layer).unwrap().properties;

				assert_eq!(p.get("x").unwrap(), &GeoValue::from(coord.x));
				assert_eq!(p.get("y").unwrap(), &GeoValue::from(coord.y));
				assert_eq!(p.get("z").unwrap(), &GeoValue::from(coord.z));

				p.get("filename").unwrap().to_string()
			})
			.join(",")
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

		error("from_vectortiles_merged").await;
		error("from_vectortiles_merged [ ]").await;
		error("from_vectortiles_merged [ from_container filename=1 ]").await;
	}

	#[tokio::test]
	async fn test_operation_get_tile_data() -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let result = factory
			.operation_from_vpl("from_vectortiles_merged [ from_container filename=1, from_container filename=2 ]")
			.await?;

		let coord = TileCoord3::new(1, 2, 3)?;
		let blob = result.get_tile_data(&coord).await?.unwrap();

		assert_eq!(check_tile(&blob, &coord), "1,2");

		Ok(())
	}

	#[tokio::test]
	async fn test_operation_get_tile_stream() -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let result = factory
			.operation_from_vpl(
				r#"from_vectortiles_merged [
					from_container filename="A" | filter_bbox bbox=[-180,-45,90,85],
					from_container filename="B" | filter_bbox bbox=[-90,-85,180,45]
				]"#,
			)
			.await?;

		let bbox = TileBBox::new_full(3)?;
		let tiles = result.get_tile_stream(bbox).await?.collect().await;

		assert_eq!(
			arrange_tiles(tiles, |coord, blob| {
				match check_tile(&blob, &coord).as_str() {
					"A" => "🟦",
					"B" => "🟨",
					"A,B" => "🟩",
					e => panic!("{}", e),
				}
			}),
			vec![
				"🟦 🟦 🟦 🟦 🟦 🟦 ❌ ❌",
				"🟦 🟦 🟦 🟦 🟦 🟦 ❌ ❌",
				"🟦 🟦 🟩 🟩 🟩 🟩 🟨 🟨",
				"🟦 🟦 🟩 🟩 🟩 🟩 🟨 🟨",
				"🟦 🟦 🟩 🟩 🟩 🟩 🟨 🟨",
				"🟦 🟦 🟩 🟩 🟩 🟩 🟨 🟨",
				"❌ ❌ 🟨 🟨 🟨 🟨 🟨 🟨",
				"❌ ❌ 🟨 🟨 🟨 🟨 🟨 🟨"
			]
		);

		Ok(())
	}

	#[tokio::test]
	async fn test_operation_parameters() -> Result<()> {
		let factory = PipelineFactory::default(
			Path::new(""),
			Box::new(|filename: String| -> BoxFuture<Result<Box<dyn TilesReaderTrait>>> {
				Box::pin(async move {
					let mut pyramide = TileBBoxPyramid::new_empty();
					for c in filename.chars() {
						pyramide.include_bbox(&TileBBox::new_full(c.to_digit(10).unwrap() as u8)?);
					}
					Ok(Box::new(MockVectorSource::new(
						&[("mock", &[&[("filename", &filename)]])],
						Some(pyramide),
					)) as Box<dyn TilesReaderTrait>)
				})
			}),
		);

		let result = factory
			.operation_from_vpl(
				r#"from_vectortiles_merged [ from_container filename="12", from_container filename="23" ]"#,
			)
			.await?;

		let parameters = result.get_parameters();

		assert_eq!(parameters.tile_format, TileFormat::MVT);
		assert_eq!(parameters.tile_compression, TileCompression::Uncompressed);
		assert_eq!(
			format!("{}", parameters.bbox_pyramid),
			"[1: [0,0,1,1] (4), 2: [0,0,3,3] (16), 3: [0,0,7,7] (64)]"
		);

		for level in 0..=4 {
			assert!(
				result
					.get_tile_data(&TileCoord3::new(0, 0, level)?)
					.await?
					.is_some()
					.bitxor(!(1..=3).contains(&level)),
				"level: {level}"
			);
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_merge_tiles_multiple_layers() -> Result<()> {
		let blob1 = VectorTile::new(vec![VectorTileLayer::new_standard("layer1")]);
		let blob2 = VectorTile::new(vec![VectorTileLayer::new_standard("layer2")]);

		let merged_tile = merge_vector_tiles(vec![blob1, blob2])?;

		assert_eq!(merged_tile.layers.len(), 2);
		assert!(merged_tile.layers.iter().any(|l| l.name == "layer1"));
		assert!(merged_tile.layers.iter().any(|l| l.name == "layer2"));

		Ok(())
	}
}
