use crate::{
	containers::{TileConverterBox, TileConverterTrait, TileReaderBox},
	shared::{compress, Compression, ProgressBar, TileConverterConfig, TileFormat},
};
use anyhow::Result;
use async_trait::async_trait;
use futures_util::StreamExt;
use log::trace;
use std::{
	fs::File,
	path::{Path, PathBuf},
};
use tar::{Builder, Header};
use tokio::sync::Mutex;

pub struct TileConverter {
	builder: Builder<File>,
	config: TileConverterConfig,
}

#[async_trait]
impl TileConverterTrait for TileConverter {
	async fn new(filename: &str, config: TileConverterConfig) -> Result<TileConverterBox>
	where
		Self: Sized,
	{
		trace!("new {:?}", filename);

		let file = File::create(filename)?;
		let builder = Builder::new(file);

		Ok(Box::new(TileConverter { builder, config }))
	}
	async fn convert_from(&mut self, reader: &mut TileReaderBox) -> Result<()> {
		trace!("convert_from");

		self.config.finalize_with_parameters(reader.get_parameters()?);

		let tile_converter = self.config.get_tile_recompressor();

		let ext_form = match self.config.get_tile_format() {
			TileFormat::BIN => "",

			TileFormat::PNG => ".png",
			TileFormat::JPG => ".jpg",
			TileFormat::WEBP => ".webp",
			TileFormat::AVIF => ".avif",
			TileFormat::SVG => ".svg",

			TileFormat::PBF => ".pbf",
			TileFormat::GEOJSON => ".geojson",
			TileFormat::TOPOJSON => ".topojson",
			TileFormat::JSON => ".json",
		};

		let ext_comp = match self.config.get_tile_compression() {
			Compression::None => "",
			Compression::Gzip => ".gz",
			Compression::Brotli => ".br",
		};

		let bbox_pyramid = self.config.get_bbox_pyramid();

		let meta_data_option = reader.get_meta().await?;

		if let Some(meta_data) = meta_data_option {
			let meta_data = compress(meta_data, self.config.get_tile_compression())?;
			let filename = format!("tiles.json{}", ext_comp);

			let mut header = Header::new_gnu();
			header.set_size(meta_data.len() as u64);
			header.set_mode(0o644);

			self
				.builder
				.append_data(&mut header, Path::new(&filename), meta_data.as_slice())?;
		}

		let mut bar = ProgressBar::new("converting tiles", bbox_pyramid.count_tiles());
		let mutex_bar = &Mutex::new(&mut bar);
		let mutex_builder = &Mutex::new(&mut self.builder);

		for bbox in bbox_pyramid.iter_levels() {
			let mut stream = reader.get_bbox_tile_stream(*bbox).await;

			while let Some(entry) = stream.next().await {
				let (coord, blob) = entry;
				mutex_bar.lock().await.inc(1);

				if let Ok(blob) = tile_converter.process_blob(blob) {
					let filename = format!(
						"./{}/{}/{}{}{}",
						coord.get_z(),
						coord.get_y(),
						coord.get_x(),
						ext_form,
						ext_comp
					);
					let path = PathBuf::from(&filename);

					// Build header
					let mut header = Header::new_gnu();
					header.set_size(blob.len() as u64);
					header.set_mode(0o644);

					// Write blob to file
					mutex_builder
						.lock()
						.await
						.append_data(&mut header, path, blob.as_slice())?;
				}
			}
		}

		bar.finish();
		self.builder.finish()?;

		Ok(())
	}
}
