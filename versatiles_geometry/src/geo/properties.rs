#![allow(dead_code)]

use versatiles_core::json::JsonObject;

use super::GeoValue;
use std::{
	collections::{BTreeMap, btree_map},
	fmt::Debug,
};

#[derive(Clone, PartialEq)]
pub struct GeoProperties(pub BTreeMap<String, GeoValue>);

impl Default for GeoProperties {
	fn default() -> Self {
		Self::new()
	}
}

impl GeoProperties {
	pub fn new() -> GeoProperties {
		GeoProperties(BTreeMap::new())
	}

	pub fn insert(&mut self, key: String, value: GeoValue) {
		self.0.insert(key, value);
	}

	pub fn update(&mut self, new_properties: &GeoProperties) {
		for (k, v) in new_properties.iter() {
			self.0.insert(k.to_string(), v.clone());
		}
	}

	pub fn remove(&mut self, key: &str) {
		self.0.remove(key);
	}

	pub fn clear(&mut self) {
		self.0.clear();
	}

	pub fn len(&self) -> usize {
		self.0.len()
	}

	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	pub fn get(&self, key: &str) -> Option<&GeoValue> {
		self.0.get(key)
	}

	pub fn iter(&self) -> btree_map::Iter<'_, String, GeoValue> {
		self.0.iter()
	}

	pub fn retain<F>(&mut self, f: F)
	where
		F: Fn(&String, &GeoValue) -> bool,
	{
		self.0.retain(|k, v| f(k, v));
	}

	pub fn to_json(&self) -> JsonObject {
		let mut obj = JsonObject::new();
		for (k, v) in &self.0 {
			obj.set(k, v.to_json());
		}
		obj
	}
}

impl IntoIterator for GeoProperties {
	type Item = (String, GeoValue);
	type IntoIter = btree_map::IntoIter<String, GeoValue>;
	fn into_iter(self) -> Self::IntoIter {
		self.0.into_iter()
	}
}

impl From<Vec<(&str, GeoValue)>> for GeoProperties {
	fn from(value: Vec<(&str, GeoValue)>) -> Self {
		GeoProperties(value.into_iter().map(|(k, v)| (k.to_string(), v)).collect())
	}
}

impl From<Vec<(&str, &str)>> for GeoProperties {
	fn from(value: Vec<(&str, &str)>) -> Self {
		GeoProperties(
			value
				.into_iter()
				.map(|(k, v)| (k.to_string(), GeoValue::from(v)))
				.collect(),
		)
	}
}

impl FromIterator<(String, GeoValue)> for GeoProperties {
	fn from_iter<T: IntoIterator<Item = (String, GeoValue)>>(iter: T) -> Self {
		GeoProperties(BTreeMap::from_iter(iter))
	}
}

impl Debug for GeoProperties {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let fields = self.0.clone().into_iter().collect::<Vec<(String, GeoValue)>>();

		f.debug_map().entries(fields).finish()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn gv(s: &str) -> GeoValue {
		GeoValue::from(s)
	}

	#[test]
	fn new_is_empty() {
		let p = GeoProperties::new();
		assert!(p.is_empty());
		assert_eq!(p.len(), 0);
	}

	#[test]
	fn insert_and_get() {
		let mut p = GeoProperties::new();
		p.insert("name".into(), gv("Berlin"));
		p.insert("country".into(), gv("DE"));
		assert_eq!(p.len(), 2);
		assert_eq!(p.get("name"), Some(&gv("Berlin")));
		assert_eq!(p.get("country"), Some(&gv("DE")));
		assert_eq!(p.get("missing"), None);
	}

	#[test]
	fn update_merges_and_overwrites() {
		let mut base = GeoProperties::from(vec![("a", gv("1")), ("b", gv("2"))]);
		let add = GeoProperties::from(vec![("b", gv("B")), ("c", gv("3"))]);
		base.update(&add);
		assert_eq!(base.len(), 3);
		assert_eq!(base.get("a"), Some(&gv("1")));
		assert_eq!(base.get("b"), Some(&gv("B"))); // overwritten
		assert_eq!(base.get("c"), Some(&gv("3")));
	}

	#[test]
	fn remove_and_clear() {
		let mut p = GeoProperties::from(vec![("x", gv("1")), ("y", gv("2"))]);
		p.remove("x");
		assert!(p.get("x").is_none());
		assert_eq!(p.len(), 1);
		p.clear();
		assert!(p.is_empty());
	}

	#[test]
	fn iter_is_sorted_by_key() {
		let p = GeoProperties::from(vec![("b", gv("2")), ("a", gv("1")), ("c", gv("3"))]);
		let keys: Vec<String> = p.iter().map(|(k, _)| k.clone()).collect();
		assert_eq!(keys, vec!["a", "b", "c"]);
	}

	#[test]
	fn retain_filters_entries() {
		let mut p = GeoProperties::from(vec![("keep", gv("1")), ("drop", gv("2"))]);
		p.retain(|k, _| k == "keep");
		assert_eq!(p.len(), 1);
		assert!(p.get("keep").is_some());
		assert!(p.get("drop").is_none());
	}

	#[test]
	fn to_json_smoke() {
		let p = GeoProperties::from(vec![("name", gv("Berlin")), ("country", gv("DE"))]);
		let obj = p.to_json();
		// Smoke test: object should contain both keys after serialization
		let s = format!("{}", obj);
		assert!(s.contains("name"));
		assert!(s.contains("Berlin"));
		assert!(s.contains("country"));
		assert!(s.contains("DE"));
	}

	#[test]
	fn into_iterator_consumes() {
		let p = GeoProperties::from(vec![("a", gv("1")), ("b", gv("2"))]);
		let mut pairs: Vec<(String, GeoValue)> = p.into_iter().collect();
		pairs.sort_by(|a, b| a.0.cmp(&b.0));
		assert_eq!(pairs.len(), 2);
		assert_eq!(pairs[0].0, "a");
		assert_eq!(pairs[1].0, "b");
	}

	#[test]
	fn from_vec_str_str() {
		let p = GeoProperties::from(vec![("name", "Berlin"), ("country", "DE")]);
		assert_eq!(p.get("name"), Some(&gv("Berlin")));
		assert_eq!(p.get("country"), Some(&gv("DE")));
	}

	#[test]
	fn from_iterator_trait() {
		let items: Vec<(String, GeoValue)> = vec![("a".into(), gv("1")), ("b".into(), gv("2"))];
		let p = items.into_iter().collect::<GeoProperties>();
		assert_eq!(p.len(), 2);
		assert!(p.get("a").is_some());
		assert!(p.get("b").is_some());
	}

	#[test]
	fn debug_includes_keys_and_values() {
		let p = GeoProperties::from(vec![("city", gv("Berlin"))]);
		let dbg = format!("{:?}", p);
		assert!(dbg.contains("city"));
		assert!(dbg.contains("Berlin"));
	}
}
