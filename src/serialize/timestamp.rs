use serde::ser::{Serialize, SerializeMap, SerializeSeq, Serializer};

use crate::serialize::transform::{
    TransformMap, TransformStructAsMap, TransformStructVariantAsMap, TransformTuple,
    TransformTupleStruct, TransformTupleVariant,
};
use crate::serialize::{FieldKind, delegate_simple, timestamp_to_rfc3339, try_parse_unix_string};

pub(crate) struct TimestampValue<'a, T: ?Sized> {
    pub(crate) value: &'a T,
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
    type SerializeSeq = TimestampSeq<S::SerializeSeq>;
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
    fn serialize_str(self, v: &str) -> Result<Self::Ok, Self::Error> {
        match try_parse_unix_string(v) {
            Some(ts) => self.inner.serialize_str(&timestamp_to_rfc3339(ts)),
            None => self.inner.serialize_str(v),
        }
    }

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
    delegate_simple!(serialize_bytes, &[u8]);

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_none()
    }
    fn serialize_some<T: Serialize + ?Sized>(self, v: &T) -> Result<Self::Ok, Self::Error> {
        self.inner.serialize_some(&TimestampValue { value: v })
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
            .serialize_newtype_struct(n, &TimestampValue { value: v })
    }
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        n: &'static str,
        vi: u32,
        variant: &'static str,
        v: &T,
    ) -> Result<Self::Ok, Self::Error> {
        self.inner
            .serialize_newtype_variant(n, vi, variant, &TimestampValue { value: v })
    }
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(TimestampSeq {
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

/// Sequence serializer that keeps timestamp semantics for each element,
/// so arrays of unix-seconds strings (e.g. `trade_date: ["1776752384", ...]`)
/// get converted element by element.
pub(crate) struct TimestampSeq<S> {
    pub(crate) inner: S,
}

impl<S: SerializeSeq> SerializeSeq for TimestampSeq<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.inner.serialize_element(&TimestampValue { value })
    }
    fn end(self) -> Result<Self::Ok, Self::Error> {
        self.inner.end()
    }
}
