//! Custom Serializer wrapper that transforms JSON output during serialization:
//! - Field names → snake_case
//! - Fields ending with `_at` containing i64/u64 → RFC3339 UTC string
//! - Field `counter_id` (string) → renamed to `symbol`, value converted
//! - Field `counter_ids` (array of strings) → renamed to `symbols`, each converted
//!
//! Zero intermediate allocation for SDK types (`to_tool_json`).

use serde::ser::{
    self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

use crate::counter::counter_id_to_symbol;

/// Serialize a Rust value with field transformations, zero intermediate Value.
pub fn to_tool_json(value: &impl Serialize) -> Result<String, serde_json::Error> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::new(&mut buf);
    value.serialize(TransformSerializer { inner: &mut ser })?;
    Ok(String::from_utf8(buf).expect("serde_json produces valid UTF-8"))
}

/// Stream-transcode raw JSON bytes with field transformations.
/// No intermediate `serde_json::Value` allocation — reads tokens from input
/// and writes transformed tokens directly to output.
pub fn transform_json(input: &[u8]) -> Result<String, serde_json::Error> {
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::new(&mut buf);
    let mut de = serde_json::Deserializer::from_slice(input);
    serde_transcode::transcode(&mut de, TransformSerializer { inner: &mut ser })?;
    Ok(String::from_utf8(buf).expect("serde_json produces valid UTF-8"))
}

fn to_snake_case(s: &str) -> String {
    let mut result = String::with_capacity(s.len() + 4);
    for (i, c) in s.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                result.push('_');
            }
            result.push(c.to_lowercase().next().unwrap());
        } else {
            result.push(c);
        }
    }
    result
}

fn timestamp_to_rfc3339(ts: i64) -> String {
    use time::OffsetDateTime;
    match OffsetDateTime::from_unix_timestamp(ts) {
        Ok(dt) => dt
            .format(&time::format_description::well_known::Rfc3339)
            .unwrap_or_else(|_| ts.to_string()),
        Err(_) => ts.to_string(),
    }
}

#[derive(Clone, Copy, PartialEq)]
enum FieldKind {
    Normal,
    Timestamp,
    CounterId,
    CounterIds,
}

fn classify_field(snake_name: &str) -> FieldKind {
    if snake_name == "counter_id" {
        FieldKind::CounterId
    } else if snake_name == "counter_ids" {
        FieldKind::CounterIds
    } else if snake_name.ends_with("_at") {
        FieldKind::Timestamp
    } else {
        FieldKind::Normal
    }
}

fn output_key(snake_name: &str, kind: FieldKind) -> &str {
    match kind {
        FieldKind::CounterId => "symbol",
        FieldKind::CounterIds => "symbols",
        _ => snake_name,
    }
}

struct Transformed<'a, T: ?Sized> {
    value: &'a T,
}

impl<T: Serialize + ?Sized> Serialize for Transformed<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value
            .serialize(TransformSerializer { inner: serializer })
    }
}

fn key_to_string<T: Serialize + ?Sized>(key: &T) -> Result<String, String> {
    let s = serde_json::to_string(key).map_err(|e| e.to_string())?;
    // Remove quotes from JSON string: "foo" -> foo
    Ok(if s.starts_with('"') && s.ends_with('"') {
        s[1..s.len() - 1].to_string()
    } else {
        s
    })
}

pub struct TransformSerializer<S> {
    inner: S,
}

macro_rules! delegate_simple {
    ($method:ident, $ty:ty) => {
        fn $method(self, v: $ty) -> Result<Self::Ok, Self::Error> {
            self.inner.$method(v)
        }
    };
}

impl<S: Serializer> Serializer for TransformSerializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = TransformSeq<S::SerializeSeq>;
    type SerializeTuple = TransformTuple<S::SerializeTuple>;
    type SerializeTupleStruct = TransformTupleStruct<S::SerializeTupleStruct>;
    type SerializeTupleVariant = TransformTupleVariant<S::SerializeTupleVariant>;
    type SerializeMap = TransformMap<S::SerializeMap>;
    // Use SerializeMap underneath for struct too, so we can use dynamic key strings
    type SerializeStruct = TransformStructAsMap<S::SerializeMap>;
    type SerializeStructVariant = TransformStructVariantAsMap<S::SerializeMap>;

    delegate_simple!(serialize_bool, bool);
    delegate_simple!(serialize_i8, i8);
    delegate_simple!(serialize_i16, i16);
    delegate_simple!(serialize_i32, i32);
    delegate_simple!(serialize_i64, i64);
    delegate_simple!(serialize_u8, u8);
    delegate_simple!(serialize_u16, u16);
    delegate_simple!(serialize_u32, u32);
    delegate_simple!(serialize_u64, u64);
    delegate_simple!(serialize_f32, f32);
    delegate_simple!(serialize_f64, f64);
    delegate_simple!(serialize_char, char);
    delegate_simple!(serialize_str, &str);
    delegate_simple!(serialize_bytes, &[u8]);

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_none()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_some(&Transformed { value })
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit()
    }
    fn serialize_unit_struct(self, name: &'static str) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_struct(name)
    }
    fn serialize_unit_variant(
        self,
        name: &'static str,
        vi: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_variant(name, vi, variant)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_struct(name, &Transformed { value })
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        name: &'static str,
        vi: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_variant(name, vi, variant, &Transformed { value })
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(TransformSeq {
            inner: self.inner.serialize_seq(len)?,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(TransformTuple {
            inner: self.inner.serialize_tuple(len)?,
        })
    }
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(TransformTupleStruct {
            inner: self.inner.serialize_tuple_struct(name, len)?,
        })
    }
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        vi: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(TransformTupleVariant {
            inner: self.inner.serialize_tuple_variant(name, vi, variant, len)?,
        })
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(TransformMap {
            inner: self.inner.serialize_map(len)?,
            current_kind: FieldKind::Normal,
        })
    }
    fn serialize_struct(
        self,
        _name: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        // Serialize struct as map to allow dynamic key names
        Ok(TransformStructAsMap {
            inner: self.inner.serialize_map(Some(len))?,
        })
    }
    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _vi: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        // Serialize as {"variant": {fields...}}
        let mut map = self.inner.serialize_map(Some(1))?;
        map.serialize_key(variant)?;
        // We need a nested map for fields - but SerializeMap doesn't support nesting directly.
        // Use a workaround: write the variant key, then the inner struct as value.
        // Actually, we'll handle this by writing variant as a map entry later.
        // For simplicity, flatten: the variant serializer will collect fields.
        Ok(TransformStructVariantAsMap {
            outer: map,
            fields: Vec::with_capacity(len),
        })
    }
}

pub struct TransformSeq<S> {
    inner: S,
}

impl<S: SerializeSeq> SerializeSeq for TransformSeq<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.inner.serialize_element(&Transformed { value })
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

pub struct TransformTuple<S> {
    inner: S,
}

impl<S: SerializeTuple> SerializeTuple for TransformTuple<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.inner.serialize_element(&Transformed { value })
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

pub struct TransformTupleStruct<S> {
    inner: S,
}

impl<S: SerializeTupleStruct> SerializeTupleStruct for TransformTupleStruct<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.inner.serialize_field(&Transformed { value })
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

pub struct TransformTupleVariant<S> {
    inner: S,
}

impl<S: SerializeTupleVariant> SerializeTupleVariant for TransformTupleVariant<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_field<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.inner.serialize_field(&Transformed { value })
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

pub struct TransformMap<M> {
    inner: M,
    current_kind: FieldKind,
}

impl<M: SerializeMap> SerializeMap for TransformMap<M> {
    type Ok = M::Ok;
    type Error = M::Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        let raw = key_to_string(key).map_err(ser::Error::custom)?;
        let snake = to_snake_case(&raw);
        let kind = classify_field(&snake);
        self.current_kind = kind;
        let out = output_key(&snake, kind);
        self.inner.serialize_key(out)
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        match self.current_kind {
            FieldKind::Timestamp => self.inner.serialize_value(&TimestampValue { value }),
            FieldKind::CounterId => self.inner.serialize_value(&CounterIdValue { value }),
            FieldKind::CounterIds => self.inner.serialize_value(&CounterIdsValue { value }),
            FieldKind::Normal => self.inner.serialize_value(&Transformed { value }),
        }
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

pub struct TransformStructAsMap<M> {
    inner: M,
}

impl<M: SerializeMap> SerializeStruct for TransformStructAsMap<M> {
    type Ok = M::Ok;
    type Error = M::Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        let snake = to_snake_case(key);
        let kind = classify_field(&snake);
        let out_key = match kind {
            FieldKind::CounterId => "symbol",
            FieldKind::CounterIds => "symbols",
            _ => &snake,
        };
        self.inner.serialize_key(out_key)?;
        match kind {
            FieldKind::Timestamp => self.inner.serialize_value(&TimestampValue { value }),
            FieldKind::CounterId => self.inner.serialize_value(&CounterIdValue { value }),
            FieldKind::CounterIds => self.inner.serialize_value(&CounterIdsValue { value }),
            FieldKind::Normal => self.inner.serialize_value(&Transformed { value }),
        }
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

pub struct TransformStructVariantAsMap<M> {
    outer: M,
    fields: Vec<(String, serde_json::Value)>,
}

impl<M: SerializeMap> SerializeStructVariant for TransformStructVariantAsMap<M> {
    type Ok = M::Ok;
    type Error = M::Error;

    fn serialize_field<T: Serialize + ?Sized>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        let snake = to_snake_case(key);
        let kind = classify_field(&snake);
        let out_key = match kind {
            FieldKind::CounterId => "symbol".to_string(),
            FieldKind::CounterIds => "symbols".to_string(),
            _ => snake.clone(),
        };
        // For struct variant, collect fields then serialize as nested object
        let val = serde_json::to_value(value).map_err(ser::Error::custom)?;
        self.fields.push((out_key, val));
        Ok(())
    }

    fn end(mut self) -> Result<Self::Ok, Self::Error> {
        let obj: serde_json::Map<String, serde_json::Value> = self.fields.into_iter().collect();
        self.outer.serialize_value(&obj)?;
        self.outer.end()
    }
}

struct TimestampValue<'a, T: ?Sized> {
    value: &'a T,
}

impl<T: Serialize + ?Sized> Serialize for TimestampValue<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value
            .serialize(TimestampSerializer { inner: serializer })
    }
}

struct TimestampSerializer<S> {
    inner: S,
}

impl<S: Serializer> Serializer for TimestampSerializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = TransformSeq<S::SerializeSeq>;
    type SerializeTuple = TransformTuple<S::SerializeTuple>;
    type SerializeTupleStruct = TransformTupleStruct<S::SerializeTupleStruct>;
    type SerializeTupleVariant = TransformTupleVariant<S::SerializeTupleVariant>;
    type SerializeMap = TransformMap<S::SerializeMap>;
    type SerializeStruct = TransformStructAsMap<S::SerializeMap>;
    type SerializeStructVariant = TransformStructVariantAsMap<S::SerializeMap>;

    fn serialize_i64(self, v: i64) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_str(&timestamp_to_rfc3339(v))
    }
    fn serialize_u64(self, v: u64) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_str(&timestamp_to_rfc3339(v as i64))
    }

    // Everything else falls through to TransformSerializer behavior
    delegate_simple!(serialize_bool, bool);
    delegate_simple!(serialize_i8, i8);
    delegate_simple!(serialize_i16, i16);
    delegate_simple!(serialize_i32, i32);
    delegate_simple!(serialize_u8, u8);
    delegate_simple!(serialize_u16, u16);
    delegate_simple!(serialize_u32, u32);
    delegate_simple!(serialize_f32, f32);
    delegate_simple!(serialize_f64, f64);
    delegate_simple!(serialize_char, char);
    delegate_simple!(serialize_str, &str);
    delegate_simple!(serialize_bytes, &[u8]);

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_none()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_some(&Transformed { value: v })
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit()
    }
    fn serialize_unit_struct(self, n: &'static str) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_struct(n)
    }
    fn serialize_unit_variant(
        self,
        n: &'static str,
        vi: u32,
        v: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_variant(n, vi, v)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_struct(n, &Transformed { value: v })
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        vi: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_variant(n, vi, variant, &Transformed { value: v })
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(TransformSeq {
            inner: self.inner.serialize_seq(len)?,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(TransformTuple {
            inner: self.inner.serialize_tuple(len)?,
        })
    }
    fn serialize_tuple_struct(
        self,
        n: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(TransformTupleStruct {
            inner: self.inner.serialize_tuple_struct(n, len)?,
        })
    }
    fn serialize_tuple_variant(
        self,
        n: &'static str,
        vi: u32,
        v: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(TransformTupleVariant {
            inner: self.inner.serialize_tuple_variant(n, vi, v, len)?,
        })
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(TransformMap {
            inner: self.inner.serialize_map(len)?,
            current_kind: FieldKind::Normal,
        })
    }
    fn serialize_struct(
        self,
        _n: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(TransformStructAsMap {
            inner: self.inner.serialize_map(Some(len))?,
        })
    }
    fn serialize_struct_variant(
        self,
        _n: &'static str,
        _vi: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let mut m = self.inner.serialize_map(Some(1))?;
        m.serialize_key(variant)?;
        Ok(TransformStructVariantAsMap {
            outer: m,
            fields: Vec::with_capacity(len),
        })
    }
}

struct CounterIdValue<'a, T: ?Sized> {
    value: &'a T,
}

impl<T: Serialize + ?Sized> Serialize for CounterIdValue<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value
            .serialize(CounterIdSerializer { inner: serializer })
    }
}

struct CounterIdSerializer<S> {
    inner: S,
}

impl<S: Serializer> Serializer for CounterIdSerializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = TransformSeq<S::SerializeSeq>;
    type SerializeTuple = TransformTuple<S::SerializeTuple>;
    type SerializeTupleStruct = TransformTupleStruct<S::SerializeTupleStruct>;
    type SerializeTupleVariant = TransformTupleVariant<S::SerializeTupleVariant>;
    type SerializeMap = TransformMap<S::SerializeMap>;
    type SerializeStruct = TransformStructAsMap<S::SerializeMap>;
    type SerializeStructVariant = TransformStructVariantAsMap<S::SerializeMap>;

    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_str(&counter_id_to_symbol(v))
    }

    delegate_simple!(serialize_bool, bool);
    delegate_simple!(serialize_i8, i8);
    delegate_simple!(serialize_i16, i16);
    delegate_simple!(serialize_i32, i32);
    delegate_simple!(serialize_i64, i64);
    delegate_simple!(serialize_u8, u8);
    delegate_simple!(serialize_u16, u16);
    delegate_simple!(serialize_u32, u32);
    delegate_simple!(serialize_u64, u64);
    delegate_simple!(serialize_f32, f32);
    delegate_simple!(serialize_f64, f64);
    delegate_simple!(serialize_char, char);
    delegate_simple!(serialize_bytes, &[u8]);

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_none()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_some(&Transformed { value: v })
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit()
    }
    fn serialize_unit_struct(self, n: &'static str) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_struct(n)
    }
    fn serialize_unit_variant(
        self,
        n: &'static str,
        vi: u32,
        v: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_variant(n, vi, v)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_struct(n, &Transformed { value: v })
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        vi: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_variant(n, vi, variant, &Transformed { value: v })
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(TransformSeq {
            inner: self.inner.serialize_seq(len)?,
        })
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(TransformTuple {
            inner: self.inner.serialize_tuple(len)?,
        })
    }
    fn serialize_tuple_struct(
        self,
        n: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(TransformTupleStruct {
            inner: self.inner.serialize_tuple_struct(n, len)?,
        })
    }
    fn serialize_tuple_variant(
        self,
        n: &'static str,
        vi: u32,
        v: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(TransformTupleVariant {
            inner: self.inner.serialize_tuple_variant(n, vi, v, len)?,
        })
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(TransformMap {
            inner: self.inner.serialize_map(len)?,
            current_kind: FieldKind::Normal,
        })
    }
    fn serialize_struct(
        self,
        _n: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(TransformStructAsMap {
            inner: self.inner.serialize_map(Some(len))?,
        })
    }
    fn serialize_struct_variant(
        self,
        _n: &'static str,
        _vi: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let mut m = self.inner.serialize_map(Some(1))?;
        m.serialize_key(variant)?;
        Ok(TransformStructVariantAsMap {
            outer: m,
            fields: Vec::with_capacity(len),
        })
    }
}

struct CounterIdsValue<'a, T: ?Sized> {
    value: &'a T,
}

impl<T: Serialize + ?Sized> Serialize for CounterIdsValue<'_, T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.value
            .serialize(CounterIdsSerializer { inner: serializer })
    }
}

struct CounterIdsSerializer<S> {
    inner: S,
}

impl<S: Serializer> Serializer for CounterIdsSerializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = CounterIdsSeq<S::SerializeSeq>;
    type SerializeTuple = TransformTuple<S::SerializeTuple>;
    type SerializeTupleStruct = TransformTupleStruct<S::SerializeTupleStruct>;
    type SerializeTupleVariant = TransformTupleVariant<S::SerializeTupleVariant>;
    type SerializeMap = TransformMap<S::SerializeMap>;
    type SerializeStruct = TransformStructAsMap<S::SerializeMap>;
    type SerializeStructVariant = TransformStructVariantAsMap<S::SerializeMap>;

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(CounterIdsSeq {
            inner: self.inner.serialize_seq(len)?,
        })
    }

    // Non-seq passthrough
    delegate_simple!(serialize_bool, bool);
    delegate_simple!(serialize_i8, i8);
    delegate_simple!(serialize_i16, i16);
    delegate_simple!(serialize_i32, i32);
    delegate_simple!(serialize_i64, i64);
    delegate_simple!(serialize_u8, u8);
    delegate_simple!(serialize_u16, u16);
    delegate_simple!(serialize_u32, u32);
    delegate_simple!(serialize_u64, u64);
    delegate_simple!(serialize_f32, f32);
    delegate_simple!(serialize_f64, f64);
    delegate_simple!(serialize_char, char);
    delegate_simple!(serialize_str, &str);
    delegate_simple!(serialize_bytes, &[u8]);
    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_none()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_some(&Transformed { value: v })
    }
    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit()
    }
    fn serialize_unit_struct(self, n: &'static str) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_struct(n)
    }
    fn serialize_unit_variant(
        self,
        n: &'static str,
        vi: u32,
        v: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_unit_variant(n, vi, v)
    }
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_struct(n, &Transformed { value: v })
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        vi: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_variant(n, vi, variant, &Transformed { value: v })
    }
    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Ok(TransformTuple {
            inner: self.inner.serialize_tuple(len)?,
        })
    }
    fn serialize_tuple_struct(
        self,
        n: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Ok(TransformTupleStruct {
            inner: self.inner.serialize_tuple_struct(n, len)?,
        })
    }
    fn serialize_tuple_variant(
        self,
        n: &'static str,
        vi: u32,
        v: &'static str,
        len: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(TransformTupleVariant {
            inner: self.inner.serialize_tuple_variant(n, vi, v, len)?,
        })
    }
    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(TransformMap {
            inner: self.inner.serialize_map(len)?,
            current_kind: FieldKind::Normal,
        })
    }
    fn serialize_struct(
        self,
        _n: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Ok(TransformStructAsMap {
            inner: self.inner.serialize_map(Some(len))?,
        })
    }
    fn serialize_struct_variant(
        self,
        _n: &'static str,
        _vi: u32,
        variant: &'static str,
        len: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        let mut m = self.inner.serialize_map(Some(1))?;
        m.serialize_key(variant)?;
        Ok(TransformStructVariantAsMap {
            outer: m,
            fields: Vec::with_capacity(len),
        })
    }
}

pub struct CounterIdsSeq<S> {
    inner: S,
}

impl<S: SerializeSeq> SerializeSeq for CounterIdsSeq<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.inner.serialize_element(&CounterIdValue { value })
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[test]
    fn snake_case_conversion() {
        assert_eq!(to_snake_case("createdAt"), "created_at");
        assert_eq!(to_snake_case("counterIds"), "counter_ids");
        assert_eq!(to_snake_case("already_snake"), "already_snake");
    }

    #[test]
    fn timestamp_field() {
        #[derive(Serialize)]
        struct Data {
            created_at: i64,
            name: String,
        }
        let d = Data {
            created_at: 1700000000,
            name: "test".to_string(),
        };
        let json = to_tool_json(&d).unwrap();
        assert!(json.contains("2023-11-14T"), "got: {json}");
        assert!(json.contains("\"name\":\"test\""), "got: {json}");
    }

    #[test]
    fn counter_id_field() {
        #[derive(Serialize)]
        struct Data {
            counter_id: String,
        }
        let d = Data {
            counter_id: "ST/US/TSLA".to_string(),
        };
        let json = to_tool_json(&d).unwrap();
        assert!(json.contains("\"symbol\":\"TSLA.US\""), "got: {json}");
        assert!(!json.contains("counter_id"), "got: {json}");
    }

    #[test]
    fn counter_ids_field() {
        #[derive(Serialize)]
        struct Data {
            counter_ids: Vec<String>,
        }
        let d = Data {
            counter_ids: vec!["ST/US/TSLA".to_string(), "ETF/US/SPY".to_string()],
        };
        let json = to_tool_json(&d).unwrap();
        assert!(json.contains("\"symbols\""), "got: {json}");
        assert!(json.contains("TSLA.US"), "got: {json}");
        assert!(json.contains("SPY.US"), "got: {json}");
    }

    #[test]
    fn transform_json_via_value() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"counterId":"ST/US/TSLA","createdAt":1700000000}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"symbol\":\"TSLA.US\""), "got: {output}");
        assert!(output.contains("2023-11-14T"), "got: {output}");
    }

    #[test]
    fn nested_objects() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"order":{"counterId":"ST/HK/700","submittedAt":1700000000}}"#)
                .unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"symbol\":\"700.HK\""), "got: {output}");
        assert!(output.contains("2023-11-14T"), "got: {output}");
    }

    #[test]
    fn array_of_objects() {
        let input: serde_json::Value =
            serde_json::from_str(r#"[{"counterId":"ST/US/AAPL"},{"counterId":"ST/HK/700"}]"#)
                .unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("AAPL.US"), "got: {output}");
        assert!(output.contains("700.HK"), "got: {output}");
    }

    #[test]
    fn camel_case_keys() {
        let input: serde_json::Value =
            serde_json::from_str(r#"{"lastPrice":100.5,"tradeVolume":1000}"#).unwrap();
        let output = to_tool_json(&input).unwrap();
        assert!(output.contains("\"last_price\""), "got: {output}");
        assert!(output.contains("\"trade_volume\""), "got: {output}");
    }
}
