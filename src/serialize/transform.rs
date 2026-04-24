use serde::ser::{
    self, Serialize, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant,
    SerializeTuple, SerializeTupleStruct, SerializeTupleVariant, Serializer,
};

use crate::counter::counter_id_to_symbol;
use crate::serialize::counter_id::{CounterIdValue, CounterIdsValue};
use crate::serialize::timestamp::TimestampValue;
use crate::serialize::{
    FieldKind, Transformed, classify_field, delegate_simple, key_to_string, looks_like_counter_id,
    output_key, to_snake_case,
};

pub(crate) struct TransformSerializer<S> {
    pub(crate) inner: S,
}

impl<S: Serializer> Serializer for TransformSerializer<S> {
    type Ok = S::Ok;
    type Error = S::Error;
    type SerializeSeq = TransformSeq<S::SerializeSeq>;
    type SerializeTuple = TransformTuple<S::SerializeTuple>;
    type SerializeTupleStruct = TransformTupleStruct<S::SerializeTupleStruct>;
    type SerializeTupleVariant = TransformTupleVariant<S::SerializeTupleVariant>;
    type SerializeMap = TransformMap<S::SerializeMap>;
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
        let mut map = self.inner.serialize_map(Some(1))?;
        map.serialize_key(variant)?;
        Ok(TransformStructVariantAsMap {
            outer: map,
            fields: Vec::with_capacity(len),
        })
    }
}

pub(crate) struct TransformSeq<S> {
    pub(crate) inner: S,
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

pub(crate) struct TransformTuple<S> {
    pub(crate) inner: S,
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

pub(crate) struct TransformTupleStruct<S> {
    pub(crate) inner: S,
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

pub(crate) struct TransformTupleVariant<S> {
    pub(crate) inner: S,
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

pub(crate) struct TransformMap<M> {
    pub(crate) inner: M,
    pub(crate) current_kind: FieldKind,
}

impl<M: SerializeMap> SerializeMap for TransformMap<M> {
    type Ok = M::Ok;
    type Error = M::Error;

    fn serialize_key<T: Serialize + ?Sized>(&mut self, key: &T) -> Result<(), Self::Error> {
        let raw = key_to_string(key).map_err(ser::Error::custom)?;
        // If the key itself is a counter_id value (e.g. `ST/US/AAPL`),
        // convert it to a symbol and skip snake_case conversion, which would
        // otherwise insert underscores before each uppercase letter and mangle
        // the key into `s_t/_u_s/_a_a_p_l`.
        if looks_like_counter_id(&raw) {
            self.current_kind = FieldKind::Normal;
            let symbol = counter_id_to_symbol(&raw);
            return self.inner.serialize_key(&symbol);
        }
        let snake = to_snake_case(&raw);
        let kind = classify_field(&snake);
        self.current_kind = kind;
        if kind == FieldKind::Excluded {
            return Ok(());
        }
        let out = output_key(&snake, kind);
        self.inner.serialize_key(out.as_ref())
    }

    fn serialize_value<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), Self::Error> {
        match self.current_kind {
            FieldKind::Excluded => Ok(()),
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

pub(crate) struct TransformStructAsMap<M> {
    pub(crate) inner: M,
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
        if kind == FieldKind::Excluded {
            return Ok(());
        }
        let out_key = output_key(&snake, kind);
        self.inner.serialize_key(out_key.as_ref())?;
        match kind {
            FieldKind::Excluded => unreachable!("handled above"),
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

pub(crate) struct TransformStructVariantAsMap<M> {
    pub(crate) outer: M,
    pub(crate) fields: Vec<(String, serde_json::Value)>,
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
        if kind == FieldKind::Excluded {
            return Ok(());
        }
        let out_key = output_key(&snake, kind).into_owned();
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
