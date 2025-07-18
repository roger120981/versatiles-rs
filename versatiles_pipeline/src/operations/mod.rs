mod filter;
mod read;
mod vector;

use crate::traits::{ReadOperationFactoryTrait, TransformOperationFactoryTrait};

pub fn get_transform_operation_factories() -> Vec<Box<dyn TransformOperationFactoryTrait>> {
	vec![
		Box::new(filter::filter_bbox::Factory {}),
		Box::new(vector::vectortiles_update_properties::Factory {}),
	]
}

pub fn get_read_operation_factories() -> Vec<Box<dyn ReadOperationFactoryTrait>> {
	vec![
		Box::new(read::from_container::Factory {}),
		Box::new(read::from_debug::Factory {}),
		Box::new(read::from_overlayed::Factory {}),
		Box::new(read::from_vectortiles_merged::Factory {}),
	]
}
