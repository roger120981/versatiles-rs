use crate::{PipelineFactory, traits::*, vpl::VPLNode};
use anyhow::Result;
use async_trait::async_trait;
use std::fmt::Debug;
use versatiles_container::Tile;
use versatiles_core::*;

#[derive(versatiles_derive::VPLDecode, Clone, Debug)]
/// Update metadata, see also https://github.com/mapbox/tilejson-spec/tree/master/3.0.0
struct Args {
	/// Attribution text.
	attribution: Option<String>,
	/// Description text.
	description: Option<String>,
	/// Fill zoom level.
	fillzoom: Option<u8>,
	/// Name text.
	name: Option<String>,
	/// tile_content, allowed values: "rgb", "rgba", "dem/mapbox", "dem/terrarium", "dem/versatiles", "openmaptiles", "shortbread@1.0", "other", "unknown"
	content: Option<TileContent>,
}

#[derive(Debug)]
struct Operation {
	source: Box<dyn OperationTrait>,
	tilejson: TileJSON,
}

impl Operation {
	async fn build(vpl_node: VPLNode, source: Box<dyn OperationTrait>, _factory: &PipelineFactory) -> Result<Operation>
	where
		Self: Sized + OperationTrait,
	{
		let args = Args::from_vpl_node(&vpl_node)?;
		let mut tilejson = source.tilejson().clone();

		if let Some(attribution) = args.attribution {
			tilejson.set_string("attribution", &attribution)?;
		}

		if let Some(description) = args.description {
			tilejson.set_string("description", &description)?;
		}

		if let Some(fillzoom) = args.fillzoom {
			tilejson.set_byte("fillzoom", fillzoom)?;
		}

		if let Some(name) = args.name {
			tilejson.set_string("name", &name)?;
		}

		if let Some(content) = args.content {
			tilejson.tile_content = Some(TileContent::try_from(content.as_str())?);
		}

		Ok(Self { source, tilejson })
	}
}

#[async_trait]
impl OperationTrait for Operation {
	fn parameters(&self) -> &TilesReaderParameters {
		self.source.parameters()
	}

	fn tilejson(&self) -> &TileJSON {
		&self.tilejson
	}

	fn traversal(&self) -> &Traversal {
		self.source.traversal()
	}

	async fn get_stream(&self, bbox: TileBBox) -> Result<TileStream<Tile>> {
		log::debug!("get_stream {:?}", bbox);
		self.source.get_stream(bbox).await
	}
}

pub struct Factory {}

impl OperationFactoryTrait for Factory {
	fn get_docs(&self) -> String {
		Args::get_docs()
	}
	fn get_tag_name(&self) -> &str {
		"meta_update"
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
		Operation::build(vpl_node, source, factory)
			.await
			.map(|op| Box::new(op) as Box<dyn OperationTrait>)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::PipelineFactory;

	fn get_str(o: &TileJSON, k: &str) -> Option<String> {
		o.as_object().get_string(k).ok().flatten()
	}
	fn get_num(o: &TileJSON, k: &str) -> Option<f64> {
		o.as_object().get_number(k).ok().flatten()
	}

	#[tokio::test]
	async fn test_meta_update_sets_fields_and_preserves_others() -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let op = factory
            .operation_from_vpl(
                "from_debug format=mvt \
                 | filter bbox=[0,0,10,10] level_min=2 level_max=7 \
                 | meta_update name=\"Test Layer\" description=\"My desc\" attribution=\"CC-BY\" fillzoom=12 content=\"shortbread@1.0\"",
            )
            .await?;

		let tj = op.tilejson();

		// Updated keys are present
		assert_eq!(get_str(tj, "name").as_deref(), Some("Test Layer"));
		assert_eq!(get_str(tj, "description").as_deref(), Some("My desc"));
		assert_eq!(get_str(tj, "attribution").as_deref(), Some("CC-BY"));
		assert_eq!(get_num(tj, "fillzoom"), Some(12.0));

		// Tile Content was parsed into typed field
		assert_eq!(tj.tile_content, Some(TileContent::try_from("shortbread@1.0")?));

		// Pre-existing zooms from the filter should remain intact
		assert_eq!(tj.as_object().get_number("minzoom")?.unwrap(), 2.0);
		assert_eq!(tj.as_object().get_number("maxzoom")?.unwrap(), 7.0);
		Ok(())
	}

	#[tokio::test]
	async fn test_meta_update_is_noop_when_no_args() -> Result<()> {
		let factory = PipelineFactory::new_dummy();
		let op1 = factory
			.operation_from_vpl("from_debug format=mvt | filter bbox=[-5,-5,5,5] level_min=1 level_max=4")
			.await?;
		let op2 = factory
			.operation_from_vpl("from_debug format=mvt | filter bbox=[-5,-5,5,5] level_min=1 level_max=4 | meta_update")
			.await?;

		let t1 = op1.tilejson().clone();
		let t2 = op2.tilejson().clone();

		// With no args, the operation should not alter TileJSON
		assert_eq!(t1, t2);
		Ok(())
	}
}
