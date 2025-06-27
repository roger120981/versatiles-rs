use crate::{traits::*, vpl::VPLNode, PipelineFactory};
use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use std::fmt::Debug;
use versatiles_core::{tilejson::TileJSON, types::*};

#[derive(versatiles_derive::VPLDecode, Clone, Debug)]
/// Filter tiles by zoom level.
struct Args {
	/// minimal zoom level
	min: Option<u8>,
	/// maximal zoom level
	max: Option<u8>,
}

#[derive(Debug)]
struct Operation {
	parameters: TilesReaderParameters,
	source: Box<dyn OperationTrait>,
	tilejson: TileJSON,
}

impl Operation {
	fn build(
		vpl_node: VPLNode,
		source: Box<dyn OperationTrait>,
		_factory: &PipelineFactory,
	) -> BoxFuture<'_, Result<Box<dyn OperationTrait>, anyhow::Error>>
	where
		Self: Sized + OperationTrait,
	{
		Box::pin(async move {
			let args = Args::from_vpl_node(&vpl_node)?;
			let mut parameters = source.get_parameters().clone();

			if let Some(min) = args.min {
				parameters.bbox_pyramid.set_zoom_min(min);
			}

			if let Some(max) = args.max {
				parameters.bbox_pyramid.set_zoom_max(max);
			}

			let mut tilejson = source.get_tilejson().clone();
			tilejson.update_from_pyramid(&parameters.bbox_pyramid);

			Ok(Box::new(Self {
				parameters,
				source,
				tilejson,
			}) as Box<dyn OperationTrait>)
		})
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
		if self.parameters.bbox_pyramid.contains_coord(coord) {
			self.source.get_tile_data(coord).await
		} else {
			Ok(None)
		}
	}

	async fn get_tile_stream(&self, mut bbox: TileBBox) -> TileStream {
		bbox.intersect_pyramid(&self.parameters.bbox_pyramid).unwrap();
		self.source.get_tile_stream(bbox).await
	}
}

pub struct Factory {}

impl OperationFactoryTrait for Factory {
	fn get_docs(&self) -> String {
		Args::get_docs()
	}
	fn get_tag_name(&self) -> &str {
		"filter_zoom"
	}
}

#[async_trait]
impl TransformOperationFactoryTrait for Factory {
	async fn build<'a>(
		&self,
		vpl_node: VPLNode,
		source: Box<dyn OperationTrait>,
		factory: &'a PipelineFactory,
	) -> Result<Box<dyn OperationTrait>> {
		Operation::build(vpl_node, source, factory).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	async fn test_filter_zoom(min: Option<u8>, max: Option<u8>, tests: Vec<(u32, bool)>) -> Result<()> {
		let factory = PipelineFactory::new_dummy();

		let vpl = format!(
			"from_debug format=mvt | filter_zoom{}{}",
			min.map_or_else(String::new, |m| format!(" min={m}")),
			max.map_or_else(String::new, |m| format!(" max={m}")),
		);

		let operation = factory.operation_from_vpl(&vpl).await?;

		for (z, expected) in tests.into_iter() {
			let coord = TileCoord3::new(z, z, z as u8)?;
			let result = operation.get_tile_data(&coord).await?;
			if expected {
				assert!(
					result.is_some(),
					"Expected tile data for {coord:?} with min={min:?} max={max:?}"
				);
			} else {
				assert!(
					result.is_none(),
					"Expected no tile data for {coord:?} with min={min:?} max={max:?}"
				);
			}
		}

		Ok(())
	}

	#[tokio::test]
	async fn test_filter_zoom_inside() {
		let tests = vec![(0, false), (1, true), (2, true), (3, true), (4, false)];
		test_filter_zoom(Some(1), Some(3), tests).await.unwrap();
	}

	#[tokio::test]
	async fn test_filter_zoom_no_min() {
		let tests = vec![(0, true), (1, true), (2, true), (3, true), (4, false)];
		test_filter_zoom(None, Some(3), tests).await.unwrap();
	}

	#[tokio::test]
	async fn test_filter_zoom_no_max() {
		let tests = vec![(0, false), (1, true), (2, true), (3, true), (4, true)];
		test_filter_zoom(Some(1), None, tests).await.unwrap();
	}
}
