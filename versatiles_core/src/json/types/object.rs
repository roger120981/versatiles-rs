use crate::json::*;
use anyhow::Result;
use std::{collections::BTreeMap, fmt::Debug};

#[derive(Clone, Default, PartialEq)]
pub struct JsonObject(pub BTreeMap<String, JsonValue>);

impl JsonObject {
	pub fn assign(&mut self, object: JsonObject) -> Result<()> {
		for entry in object.0.into_iter() {
			self.0.insert(entry.0, entry.1);
		}
		Ok(())
	}

	pub fn get(&self, key: &str) -> Option<&JsonValue> {
		self.0.get(key)
	}

	pub fn get_string(&self, key: &str) -> Result<Option<String>> {
		self.get(key).map(JsonValue::as_string).transpose()
	}

	pub fn get_number<T>(&self, key: &str) -> Result<Option<T>>
	where
		T: AsNumber<T>,
	{
		self.get(key).map(JsonValue::as_number).transpose()
	}

	pub fn get_array(&self, key: &str) -> Result<Option<&JsonArray>> {
		self.get(key).map(JsonValue::as_array).transpose()
	}

	pub fn get_string_vec(&self, key: &str) -> Result<Option<Vec<String>>> {
		self.get_array(key)?.map(|array| array.as_string_vec()).transpose()
	}

	pub fn get_number_vec<T>(&self, key: &str) -> Result<Option<Vec<T>>>
	where
		T: AsNumber<T>,
	{
		self.get_array(key)?.map(|array| array.as_number_vec::<T>()).transpose()
	}

	pub fn get_number_array<T, const N: usize>(&self, key: &str) -> Result<Option<[T; N]>>
	where
		T: AsNumber<T>,
	{
		self
			.get_array(key)?
			.map(|array| array.as_number_array::<T, N>())
			.transpose()
	}

	pub fn set<T: Clone>(&mut self, key: &str, value: T)
	where
		JsonValue: From<T>,
	{
		self.0.insert(key.to_owned(), JsonValue::from(value));
	}

	pub fn set_optional<T>(&mut self, key: &str, value: &Option<T>)
	where
		JsonValue: From<T>,
		T: Clone,
	{
		if let Some(v) = value {
			self.0.insert(key.to_owned(), JsonValue::from(v.clone()));
		}
	}

	pub fn stringify(&self) -> String {
		let items = self
			.0
			.iter()
			.map(|(key, value)| format!("\"{}\":{}", escape_json_string(key), stringify(value)))
			.collect::<Vec<_>>();
		format!("{{{}}}", items.join(","))
	}

	pub fn stringify_pretty_single_line(&self) -> String {
		let items = self
			.0
			.iter()
			.map(|(key, value)| {
				format!(
					"\"{}\": {}",
					escape_json_string(key),
					stringify_pretty_single_line(value)
				)
			})
			.collect::<Vec<_>>();
		format!("{{ {} }}", items.join(", "))
	}

	pub fn stringify_pretty_multi_line(&self, max_width: usize, depth: usize) -> String {
		let indent = "  ".repeat(depth);
		let items = self
			.0
			.iter()
			.map(|(key, value)| {
				let key_string = format!("{}  \"{}\": ", indent, escape_json_string(key));
				format!(
					"{key_string}{}",
					stringify_pretty_multi_line(value, max_width, depth + 1, key_string.len())
				)
			})
			.collect::<Vec<_>>();
		format!("{{\n{}\n{}}}", items.join(",\n"), indent)
	}

	pub fn parse_str(json: &str) -> Result<JsonObject> {
		JsonValue::parse_str(json)?.to_object()
	}

	pub fn iter(&self) -> impl Iterator<Item = (&String, &JsonValue)> {
		self.0.iter()
	}
}

impl Debug for JsonObject {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{:?}", self.0)
	}
}

impl<T> From<Vec<(&str, T)>> for JsonValue
where
	JsonValue: From<T>,
{
	fn from(input: Vec<(&str, T)>) -> Self {
		JsonValue::Object(JsonObject::from(input))
	}
}

impl<T> From<Vec<(&str, T)>> for JsonObject
where
	JsonValue: From<T>,
{
	fn from(input: Vec<(&str, T)>) -> Self {
		JsonObject(BTreeMap::from_iter(
			input
				.into_iter()
				.map(|(key, value)| (key.to_string(), JsonValue::from(value))),
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_object_assign() {
		let mut obj1 = JsonObject::from(vec![("key1", "value1")]);
		let obj2 = JsonObject::from(vec![("key2", "value2"), ("key3", "value3")]);
		obj1.assign(obj2).unwrap();

		assert_eq!(
			obj1,
			JsonObject::from(vec![("key1", "value1"), ("key2", "value2"), ("key3", "value3"),])
		);
	}

	#[test]
	fn test_object_set_key_value() {
		let mut obj = JsonObject::default();
		obj.set("key", JsonValue::from("value"));

		assert_eq!(
			obj,
			JsonObject(BTreeMap::from_iter(vec![("key".to_string(), JsonValue::from("value"))]))
		);
	}

	#[test]
	fn test_get() {
		let obj = JsonObject::from(vec![("key", "value")]);

		assert_eq!(obj.get("key"), Some(&JsonValue::from("value")));
	}

	#[test]
	fn test_get_string() {
		let obj = JsonObject::from(vec![("key", "value")]);
		let value = obj.get_string("key").unwrap();

		assert_eq!(value, Some("value".to_string()));

		let missing = obj.get_string("missing").unwrap();
		assert_eq!(missing, None);
	}

	#[test]
	fn test_get_number() {
		let obj = JsonObject::from(vec![("key", 42)]);
		let value: Option<u8> = obj.get_number("key").unwrap();

		assert_eq!(value, Some(42));

		let missing: Option<u8> = obj.get_number("missing").unwrap();
		assert_eq!(missing, None);
	}

	#[test]
	fn test_get_array() {
		let array = JsonArray(vec![JsonValue::from("item1"), JsonValue::from("item2")]);
		let obj = JsonObject::from(vec![("key", JsonValue::Array(array.clone()))]);

		let value = obj.get_array("key").unwrap();
		assert_eq!(value, Some(&array));
	}

	#[test]
	fn test_get_string_vec() {
		let array = JsonArray(vec![JsonValue::from("item1"), JsonValue::from("item2")]);
		let obj = JsonObject::from(vec![("key", JsonValue::Array(array))]);

		let value = obj.get_string_vec("key").unwrap();
		assert_eq!(value, Some(vec!["item1".to_string(), "item2".to_string()]));
	}

	#[test]
	fn test_get_number_vec() {
		let array = JsonArray::from(vec![1, 2, 3]);
		let obj = JsonObject::from(vec![("key", JsonValue::Array(array))]);

		let value: Option<Vec<u8>> = obj.get_number_vec("key").unwrap();
		assert_eq!(value, Some(vec![1, 2, 3]));
	}

	#[test]
	fn test_get_number_array() {
		let array = JsonArray::from(vec![1, 2, 3]);
		let obj = JsonObject::from(vec![("key", JsonValue::Array(array))]);

		let value: Option<[u8; 3]> = obj.get_number_array("key").unwrap();
		assert_eq!(value, Some([1, 2, 3]));
	}

	#[test]
	fn test_set_and_set_optional() {
		let mut obj = JsonObject::default();
		obj.set("key1", 42);
		obj.set_optional("key2", &Some(84));
		obj.set_optional::<i32>("key3", &None);

		assert_eq!(
			obj,
			JsonObject(BTreeMap::from_iter(vec![
				("key1".to_string(), JsonValue::from(42)),
				("key2".to_string(), JsonValue::from(84)),
			]))
		);
	}

	#[test]
	fn test_stringify() {
		let obj = JsonObject::from(vec![
			("key1", JsonValue::from("value1")),
			("key2", JsonValue::from(42)),
			("key3", JsonValue::from(vec![1, 2])),
		]);

		let json_string = obj.stringify();
		let expected = r#"{"key1":"value1","key2":42,"key3":[1,2]}"#;

		assert_eq!(json_string, expected);
	}

	#[test]
	fn test_parse_str() {
		let json = r#"{"key1":"value1","key2":42,"key3":[1,2]}"#;
		let parsed = JsonObject::parse_str(json).unwrap();

		let expected = JsonObject::from(vec![
			("key1", JsonValue::from("value1")),
			("key2", JsonValue::from(42)),
			("key3", JsonValue::from(vec![1, 2])),
		]);

		assert_eq!(parsed, expected);
	}

	#[test]
	fn test_stringify_pretty_single_line() {
		let obj = JsonObject::from(vec![("key1", JsonValue::from("value1")), ("key2", JsonValue::from(2))]);
		let s = obj.stringify_pretty_single_line();
		assert_eq!(s, "{ \"key1\": \"value1\", \"key2\": 2 }");
	}

	#[test]
	fn test_stringify_pretty_multi_line() {
		let obj = JsonObject::from(vec![("a", JsonValue::from(1)), ("b", JsonValue::from(2))]);
		let s = obj.stringify_pretty_multi_line(80, 0);
		let expected = "{\n  \"a\": 1,\n  \"b\": 2\n}";
		assert_eq!(s, expected);
	}

	#[test]
	fn test_iter_and_order() {
		let obj = JsonObject::from(vec![("x", "y"), ("z", "w")]);
		let pairs: Vec<(&String, &JsonValue)> = obj.iter().collect();
		let keys: Vec<&String> = pairs.iter().map(|(k, _)| *k).collect();
		assert_eq!(keys, vec![&"x".to_string(), &"z".to_string()]);
	}

	#[test]
	fn test_debug_fmt() {
		let obj = JsonObject::from(vec![("k", 1)]);
		let debug_str = format!("{:?}", obj);
		let expected_map: std::collections::BTreeMap<_, _> =
			vec![("k".to_string(), JsonValue::from(1))].into_iter().collect();
		assert_eq!(debug_str, format!("{:?}", expected_map));
	}

	#[test]
	fn test_from_vec_for_jsonvalue() {
		let input = vec![("foo", 3), ("bar", 4)];
		let jv: JsonValue = input.clone().into();
		if let JsonValue::Object(obj) = jv {
			assert_eq!(obj.get_number::<i32>("foo").unwrap(), Some(3));
			assert_eq!(obj.get_number::<i32>("bar").unwrap(), Some(4));
		} else {
			panic!("Expected JsonValue::Object variant");
		}
	}

	#[test]
	fn test_get_missing_variants() {
		let obj = JsonObject::default();
		assert_eq!(obj.get_array("missing").unwrap(), None);
		assert_eq!(obj.get_string_vec("missing").unwrap(), None);
		let num_vec: Option<Vec<i32>> = obj.get_number_vec("missing").unwrap();
		assert_eq!(num_vec, None);
		let num_arr: Option<[u8; 2]> = obj.get_number_array("missing").unwrap();
		assert_eq!(num_arr, None);
	}
}
