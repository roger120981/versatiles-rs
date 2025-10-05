use super::*;
use std::fmt::Debug;
use traits::SingleGeometryTrait;

#[derive(Clone, PartialEq)]
pub struct PointGeometry(pub Coordinates);

impl PointGeometry {
	pub fn new(c: Coordinates) -> Self {
		Self(c)
	}
	pub fn x(&self) -> f64 {
		self.0.x()
	}
	pub fn y(&self) -> f64 {
		self.0.y()
	}
	pub fn as_coord(&self) -> &Coordinates {
		&self.0
	}
}

impl GeometryTrait for PointGeometry {
	fn area(&self) -> f64 {
		0.0
	}

	fn verify(&self) -> anyhow::Result<()> {
		Ok(())
	}
}

impl SingleGeometryTrait<MultiPointGeometry> for PointGeometry {
	fn into_multi(self) -> MultiPointGeometry {
		MultiPointGeometry(vec![self])
	}
}

impl Debug for PointGeometry {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		self.0.fmt(f)
	}
}

impl<T> From<T> for PointGeometry
where
	Coordinates: From<T>,
{
	fn from(value: T) -> Self {
		Self(Coordinates::from(value))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_point_geometry_new() {
		let point = PointGeometry::from(&[1, 2]);
		assert_eq!(point.x(), 1.0);
		assert_eq!(point.y(), 2.0);
	}

	#[test]
	fn test_point_geometry_eq() {
		let point1 = PointGeometry::from(&[1, 2]);
		let point2 = PointGeometry::from(&[1, 2]);
		let point3 = PointGeometry::from(&[3, 4]);
		assert_eq!(point1, point2);
		assert_ne!(point1, point3);
	}

	#[test]
	fn test_point_geometry_debug() {
		let point = PointGeometry::from(&[1, 2]);
		assert_eq!(format!("{point:?}"), "[1.0, 2.0]");
	}

	#[test]
	fn test_point_geometry_from_f64_array_ref() {
		let arr = &[1, 2];
		let point: PointGeometry = PointGeometry::from(arr);
		assert_eq!(point.x(), 1.0);
		assert_eq!(point.y(), 2.0);
	}

	#[test]
	fn test_point_geometry_from_f64_array() {
		let arr = [1.0, 2.0];
		let point: PointGeometry = PointGeometry::from(arr);
		assert_eq!(point.x(), 1.0);
		assert_eq!(point.y(), 2.0);
	}

	#[test]
	fn test_point_geometry_from_i64_array() {
		let arr = [1, 2];
		let point: PointGeometry = PointGeometry::from(&arr);
		assert_eq!(point.x(), 1.0);
		assert_eq!(point.y(), 2.0);
	}
}
