use std::fmt;
use std::marker::PhantomData;

use serde::de::{
    self, DeserializeOwned, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess,
    Visitor,
};
use serde::ser::{
    self, SerializeMap, SerializeSeq, SerializeStruct, SerializeStructVariant, SerializeTuple,
    SerializeTupleStruct, SerializeTupleVariant,
};
use serde::Serialize;

/// The exact byte distribution consumed by Mercy's entropy coder.
///
/// Its representation is intentionally private. Public prediction types may use
/// logits, fixed-point probabilities, mixtures, products, or anything else; they
/// participate in Mercy by lowering deterministically to this type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ByteCategorical {
    weights: [u64; 256],
    total: u128,
}

impl ByteCategorical {
    /// Construct an exact categorical distribution from integer frequencies.
    ///
    /// Multiplying every weight by the same positive constant produces the same
    /// mathematical distribution. Zero-weight bytes are impossible outcomes.
    pub fn from_weights(weights: [u64; 256]) -> Result<Self> {
        let total = weights.iter().map(|&weight| weight as u128).sum();
        if total == 0 {
            return Err(Error::ZeroMass {
                choices: Choices::BYTE,
            });
        }
        Ok(Self { weights, total })
    }

    /// Uniform distribution over all 256 byte values.
    pub fn uniform() -> Self {
        Self {
            weights: [1; 256],
            total: 256,
        }
    }

    /// Exact two-outcome distribution on bytes 0 and 1.
    pub fn binary(zero: u64, one: u64) -> Result<Self> {
        let mut weights = [0; 256];
        weights[0] = zero;
        weights[1] = one;
        Self::from_weights(weights)
    }

    #[inline]
    pub const fn total(&self) -> u128 {
        self.total
    }

    #[inline]
    pub const fn weight(&self, choice: u8) -> u64 {
        self.weights[choice as usize]
    }

    pub fn interval(&self, choice: u8) -> (u128, u128) {
        let index = choice as usize;
        let low = self.weights[..index]
            .iter()
            .map(|&weight| weight as u128)
            .sum::<u128>();
        (low, low + self.weights[index] as u128)
    }

    #[inline]
    pub fn probability(&self, choice: u8) -> f64 {
        self.weight(choice) as f64 / self.total as f64
    }

    /// Condition on the Serde decision domain without approximation.
    ///
    /// Mercy currently uses contiguous domains `0..n`; invalid byte values are
    /// assigned exact zero mass and the remaining integer frequencies are left
    /// untouched.
    fn restricted(&self, choices: Choices) -> Result<Self> {
        let mut weights = self.weights;
        for weight in &mut weights[choices.len() as usize..] {
            *weight = 0;
        }
        let total = weights.iter().map(|&weight| weight as u128).sum();
        if total == 0 {
            return Err(Error::ZeroMass { choices });
        }
        Ok(Self { weights, total })
    }
}

/// Lossless lowering from a user-facing prediction representation to Mercy's
/// canonical byte distribution.
///
/// This trait deliberately does not expose raw probabilities as the model ABI.
/// A prediction type owns its representation and decides how it maps to the
/// exact byte frequencies used by the coder.
pub trait IntoByteCategorical {
    fn byte_categorical(&self) -> ByteCategorical;
}

impl IntoByteCategorical for ByteCategorical {
    #[inline]
    fn byte_categorical(&self) -> ByteCategorical {
        self.clone()
    }
}

impl<P: IntoByteCategorical + ?Sized> IntoByteCategorical for &P {
    #[inline]
    fn byte_categorical(&self) -> ByteCategorical {
        (**self).byte_categorical()
    }
}

/// Stateful predictor for the canonical Mercy representation of `T`.
///
/// All contextual state lives in the model. `predict` is observational;
/// `observe` advances the model after the chosen byte-sized decision is known.
pub trait Model<T: ?Sized> {
    type Prediction<'a>: IntoByteCategorical
    where
        Self: 'a;

    fn predict(&self) -> Self::Prediction<'_>;
    fn observe(&mut self, choice: u8);
}

/// Number of alternatives in one canonical Serde decision.
///
/// Decisions are always indexed from zero and never have more than 256 choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Choices(u16);

impl Choices {
    pub const BINARY: Self = Self(2);
    pub const BYTE: Self = Self(256);

    pub const fn new(len: u16) -> Option<Self> {
        if len >= 1 && len <= 256 {
            Some(Self(len))
        } else {
            None
        }
    }

    #[inline]
    pub const fn len(self) -> u16 {
        self.0
    }

    #[inline]
    pub const fn is_empty(self) -> bool {
        false
    }

    #[inline]
    pub const fn contains(self, choice: u8) -> bool {
        (choice as u16) < self.0
    }
}

/// Entropy-encoder seam. A real range/arithmetic coder implements this trait.
pub trait ChoiceEncoder {
    fn encode(&mut self, choice: u8, distribution: &ByteCategorical) -> Result<()>;
}

/// Entropy-decoder seam. It must return one byte from `distribution`.
pub trait ChoiceDecoder {
    fn decode(&mut self, distribution: &ByteCategorical) -> Result<u8>;
}

#[derive(Debug)]
pub enum Error {
    Message(String),
    ZeroMass { choices: Choices },
    InvalidChoice { choice: u8, choices: Choices },
    ZeroProbabilityChoice(u8),
    UnknownLength(&'static str),
    LengthOverflow,
    InvalidUtf8,
    InvalidChar(u32),
    InvalidEnumVariant(u32),
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => f.write_str(message),
            Self::ZeroMass { choices } => {
                write!(
                    f,
                    "prediction assigns zero mass to all {} choices",
                    choices.len()
                )
            }
            Self::InvalidChoice { choice, choices } => {
                write!(f, "choice {choice} is outside 0..{}", choices.len())
            }
            Self::ZeroProbabilityChoice(choice) => {
                write!(f, "choice {choice} has zero probability")
            }
            Self::UnknownLength(kind) => write!(f, "{kind} must report its length"),
            Self::LengthOverflow => {
                f.write_str("length does not fit the Mercy wire representation")
            }
            Self::InvalidUtf8 => f.write_str("decoded bytes are not valid UTF-8"),
            Self::InvalidChar(value) => write!(f, "decoded invalid char ordinal {value}"),
            Self::InvalidEnumVariant(value) => write!(f, "decoded invalid enum variant {value}"),
            Self::Unsupported(what) => write!(f, "Serde operation is unsupported by Mercy: {what}"),
        }
    }
}

impl std::error::Error for Error {}

impl ser::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::Message(msg.to_string())
    }
}

impl de::Error for Error {
    fn custom<T: fmt::Display>(msg: T) -> Self {
        Self::Message(msg.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Serialize `value` by teacher-forcing the choices made by its Serde
/// representation through `model` and `coder`.
pub fn encode<T, M, C>(value: &T, model: &mut M, coder: &mut C) -> Result<()>
where
    T: Serialize,
    M: Model<T>,
    C: ChoiceEncoder,
{
    let mut serializer = Serializer::<T, M, C>::new(model, coder);
    value.serialize(&mut serializer)
}

/// Deserialize `T` by obtaining each Serde construction choice from the entropy
/// decoder under the same model used by [`encode`].
pub fn decode<T, M, C>(model: &mut M, coder: &mut C) -> Result<T>
where
    T: DeserializeOwned,
    M: Model<T>,
    C: ChoiceDecoder,
{
    let mut deserializer = Deserializer::<T, M, C>::new(model, coder);
    T::deserialize(&mut deserializer)
}

pub struct Serializer<'a, T: ?Sized, M, C> {
    model: &'a mut M,
    coder: &'a mut C,
    _type: PhantomData<fn() -> T>,
}

impl<'a, T: ?Sized, M, C> Serializer<'a, T, M, C> {
    fn new(model: &'a mut M, coder: &'a mut C) -> Self {
        Self {
            model,
            coder,
            _type: PhantomData,
        }
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceEncoder> Serializer<'_, T, M, C> {
    fn choose(&mut self, choices: Choices, choice: u8) -> Result<()> {
        if !choices.contains(choice) {
            return Err(Error::InvalidChoice { choice, choices });
        }
        let distribution = {
            let prediction = self.model.predict();
            prediction.byte_categorical().restricted(choices)?
        };
        if distribution.weight(choice) == 0 {
            return Err(Error::ZeroProbabilityChoice(choice));
        }
        self.coder.encode(choice, &distribution)?;
        self.model.observe(choice);
        Ok(())
    }

    #[inline]
    fn byte(&mut self, value: u8) -> Result<()> {
        self.choose(Choices::BYTE, value)
    }

    fn bytes(&mut self, bytes: &[u8]) -> Result<()> {
        for &byte in bytes {
            self.byte(byte)?;
        }
        Ok(())
    }

    fn length(&mut self, len: usize) -> Result<()> {
        let len = u64::try_from(len).map_err(|_| Error::LengthOverflow)?;
        self.bytes(&len.to_le_bytes())
    }
}

impl<'s, 'a, T: ?Sized, M: Model<T>, C: ChoiceEncoder> ser::Serializer
    for &'s mut Serializer<'a, T, M, C>
{
    type Ok = ();
    type Error = Error;
    type SerializeSeq = Self;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Self;
    type SerializeTupleVariant = Self;
    type SerializeMap = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Self;

    fn serialize_bool(self, value: bool) -> Result<()> {
        self.choose(Choices::BINARY, u8::from(value))
    }

    fn serialize_i8(self, value: i8) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_i16(self, value: i16) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_i32(self, value: i32) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_i64(self, value: i64) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_i128(self, value: i128) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_u8(self, value: u8) -> Result<()> {
        self.byte(value)
    }
    fn serialize_u16(self, value: u16) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_u32(self, value: u32) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_u64(self, value: u64) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_u128(self, value: u128) -> Result<()> {
        self.bytes(&value.to_le_bytes())
    }
    fn serialize_f32(self, value: f32) -> Result<()> {
        self.bytes(&value.to_bits().to_le_bytes())
    }
    fn serialize_f64(self, value: f64) -> Result<()> {
        self.bytes(&value.to_bits().to_le_bytes())
    }

    fn serialize_char(self, value: char) -> Result<()> {
        self.serialize_u32(value as u32)
    }

    fn serialize_str(self, value: &str) -> Result<()> {
        self.length(value.len())?;
        self.bytes(value.as_bytes())
    }

    fn serialize_bytes(self, value: &[u8]) -> Result<()> {
        self.length(value.len())?;
        self.bytes(value)
    }

    fn serialize_none(self) -> Result<()> {
        self.choose(Choices::BINARY, 0)
    }

    fn serialize_some<V: ?Sized + Serialize>(self, value: &V) -> Result<()> {
        self.choose(Choices::BINARY, 1)?;
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<()> {
        Ok(())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
    ) -> Result<()> {
        self.serialize_u32(variant_index)
    }

    fn serialize_newtype_struct<V: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &V,
    ) -> Result<()> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<V: ?Sized + Serialize>(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        value: &V,
    ) -> Result<()> {
        self.serialize_u32(variant_index)?;
        value.serialize(self)
    }

    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        self.length(len.ok_or(Error::UnknownLength("sequence"))?)?;
        Ok(self)
    }

    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(self)
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        Ok(self)
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        self.serialize_u32(variant_index)?;
        Ok(self)
    }

    fn serialize_map(self, len: Option<usize>) -> Result<Self::SerializeMap> {
        self.length(len.ok_or(Error::UnknownLength("map"))?)?;
        Ok(self)
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Ok(self)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        self.serialize_u32(variant_index)?;
        Ok(self)
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceEncoder> SerializeSeq for &mut Serializer<'_, T, M, C> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceEncoder> SerializeTuple for &mut Serializer<'_, T, M, C> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceEncoder> SerializeTupleStruct
    for &mut Serializer<'_, T, M, C>
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceEncoder> SerializeTupleVariant
    for &mut Serializer<'_, T, M, C>
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceEncoder> SerializeMap for &mut Serializer<'_, T, M, C> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<V: ?Sized + Serialize>(&mut self, key: &V) -> Result<()> {
        key.serialize(&mut **self)
    }

    fn serialize_value<V: ?Sized + Serialize>(&mut self, value: &V) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceEncoder> SerializeStruct for &mut Serializer<'_, T, M, C> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<V: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &V,
    ) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceEncoder> SerializeStructVariant
    for &mut Serializer<'_, T, M, C>
{
    type Ok = ();
    type Error = Error;

    fn serialize_field<V: ?Sized + Serialize>(
        &mut self,
        _key: &'static str,
        value: &V,
    ) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

pub struct Deserializer<'a, T: ?Sized, M, C> {
    model: &'a mut M,
    coder: &'a mut C,
    _type: PhantomData<fn() -> T>,
}

impl<'a, T: ?Sized, M, C> Deserializer<'a, T, M, C> {
    fn new(model: &'a mut M, coder: &'a mut C) -> Self {
        Self {
            model,
            coder,
            _type: PhantomData,
        }
    }
}

impl<T: ?Sized, M: Model<T>, C: ChoiceDecoder> Deserializer<'_, T, M, C> {
    fn choose(&mut self, choices: Choices) -> Result<u8> {
        let distribution = {
            let prediction = self.model.predict();
            prediction.byte_categorical().restricted(choices)?
        };
        let choice = self.coder.decode(&distribution)?;
        if !choices.contains(choice) {
            return Err(Error::InvalidChoice { choice, choices });
        }
        if distribution.weight(choice) == 0 {
            return Err(Error::ZeroProbabilityChoice(choice));
        }
        self.model.observe(choice);
        Ok(choice)
    }

    #[inline]
    fn byte(&mut self) -> Result<u8> {
        self.choose(Choices::BYTE)
    }

    fn bytes<const N: usize>(&mut self) -> Result<[u8; N]> {
        let mut bytes = [0; N];
        for byte in &mut bytes {
            *byte = self.byte()?;
        }
        Ok(bytes)
    }

    fn length(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes()?))
    }
}

struct SeqAccessImpl<'s, 'a, T: ?Sized, M, C> {
    de: &'s mut Deserializer<'a, T, M, C>,
    remaining: u64,
}

impl<'de, T: ?Sized, M: Model<T>, C: ChoiceDecoder> SeqAccess<'de>
    for SeqAccessImpl<'_, '_, T, M, C>
{
    type Error = Error;

    fn next_element_seed<S: DeserializeSeed<'de>>(&mut self, seed: S) -> Result<Option<S::Value>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        self.remaining -= 1;
        seed.deserialize(&mut *self.de).map(Some)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(usize::try_from(self.remaining).unwrap_or(usize::MAX))
    }
}

struct MapAccessImpl<'s, 'a, T: ?Sized, M, C> {
    de: &'s mut Deserializer<'a, T, M, C>,
    remaining: u64,
}

impl<'de, T: ?Sized, M: Model<T>, C: ChoiceDecoder> MapAccess<'de>
    for MapAccessImpl<'_, '_, T, M, C>
{
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(&mut self, seed: K) -> Result<Option<K::Value>> {
        if self.remaining == 0 {
            return Ok(None);
        }
        seed.deserialize(&mut *self.de).map(Some)
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value> {
        self.remaining -= 1;
        seed.deserialize(&mut *self.de)
    }

    fn size_hint(&self) -> Option<usize> {
        Some(usize::try_from(self.remaining).unwrap_or(usize::MAX))
    }
}

struct EnumAccessImpl<'s, 'a, T: ?Sized, M, C> {
    de: &'s mut Deserializer<'a, T, M, C>,
    index: u32,
}

struct VariantAccessImpl<'s, 'a, T: ?Sized, M, C> {
    de: &'s mut Deserializer<'a, T, M, C>,
}

impl<'de, 's, 'a, T: ?Sized, M: Model<T>, C: ChoiceDecoder> EnumAccess<'de>
    for EnumAccessImpl<'s, 'a, T, M, C>
{
    type Error = Error;
    type Variant = VariantAccessImpl<'s, 'a, T, M, C>;

    fn variant_seed<V: DeserializeSeed<'de>>(self, seed: V) -> Result<(V::Value, Self::Variant)> {
        let value = seed.deserialize(de::value::U32Deserializer::<Error>::new(self.index))?;
        Ok((value, VariantAccessImpl { de: self.de }))
    }
}

impl<'de, 's, 'a, T: ?Sized, M: Model<T>, C: ChoiceDecoder> VariantAccess<'de>
    for VariantAccessImpl<'s, 'a, T, M, C>
{
    type Error = Error;

    fn unit_variant(self) -> Result<()> {
        Ok(())
    }

    fn newtype_variant_seed<S: DeserializeSeed<'de>>(self, seed: S) -> Result<S::Value> {
        seed.deserialize(self.de)
    }

    fn tuple_variant<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        de::Deserializer::deserialize_tuple(self.de, len, visitor)
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        de::Deserializer::deserialize_tuple(self.de, fields.len(), visitor)
    }
}

impl<'de, 's, 'a, T: ?Sized, M: Model<T>, C: ChoiceDecoder> de::Deserializer<'de>
    for &'s mut Deserializer<'a, T, M, C>
{
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        Err(Error::Unsupported("deserialize_any"))
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_bool(self.choose(Choices::BINARY)? != 0)
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i8(i8::from_le_bytes(self.bytes()?))
    }
    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i16(i16::from_le_bytes(self.bytes()?))
    }
    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i32(i32::from_le_bytes(self.bytes()?))
    }
    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i64(i64::from_le_bytes(self.bytes()?))
    }
    fn deserialize_i128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_i128(i128::from_le_bytes(self.bytes()?))
    }
    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u8(self.byte()?)
    }
    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u16(u16::from_le_bytes(self.bytes()?))
    }
    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u32(u32::from_le_bytes(self.bytes()?))
    }
    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u64(u64::from_le_bytes(self.bytes()?))
    }
    fn deserialize_u128<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_u128(u128::from_le_bytes(self.bytes()?))
    }
    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_f32(f32::from_bits(u32::from_le_bytes(self.bytes()?)))
    }
    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_f64(f64::from_bits(u64::from_le_bytes(self.bytes()?)))
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let value = u32::from_le_bytes(self.bytes()?);
        let value = char::from_u32(value).ok_or(Error::InvalidChar(value))?;
        visitor.visit_char(value)
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_string(visitor)
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let len = usize::try_from(self.length()?).map_err(|_| Error::LengthOverflow)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(len)
            .map_err(|_| Error::LengthOverflow)?;
        for _ in 0..len {
            bytes.push(self.byte()?);
        }
        let string = String::from_utf8(bytes).map_err(|_| Error::InvalidUtf8)?;
        visitor.visit_string(string)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        self.deserialize_byte_buf(visitor)
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let len = usize::try_from(self.length()?).map_err(|_| Error::LengthOverflow)?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(len)
            .map_err(|_| Error::LengthOverflow)?;
        for _ in 0..len {
            bytes.push(self.byte()?);
        }
        visitor.visit_byte_buf(bytes)
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        match self.choose(Choices::BINARY)? {
            0 => visitor.visit_none(),
            1 => visitor.visit_some(self),
            choice => Err(Error::InvalidChoice {
                choice,
                choices: Choices::BINARY,
            }),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        visitor.visit_unit()
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_unit()
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let remaining = self.length()?;
        visitor.visit_seq(SeqAccessImpl {
            de: self,
            remaining,
        })
    }

    fn deserialize_tuple<V: Visitor<'de>>(self, len: usize, visitor: V) -> Result<V::Value> {
        visitor.visit_seq(SeqAccessImpl {
            de: self,
            remaining: len as u64,
        })
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        len: usize,
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_tuple(len, visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value> {
        let remaining = self.length()?;
        visitor.visit_map(MapAccessImpl {
            de: self,
            remaining,
        })
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        self.deserialize_tuple(fields.len(), visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value> {
        let index = u32::from_le_bytes(self.bytes()?);
        if index as usize >= variants.len() {
            return Err(Error::InvalidEnumVariant(index));
        }
        visitor.visit_enum(EnumAccessImpl { de: self, index })
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        Err(Error::Unsupported("deserialize_identifier"))
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value> {
        Err(Error::Unsupported("deserialize_ignored_any"))
    }

    fn is_human_readable(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Copy)]
    struct HistoryPrediction(u64);

    impl IntoByteCategorical for HistoryPrediction {
        fn byte_categorical(&self) -> ByteCategorical {
            let weights = std::array::from_fn(|choice| {
                let choice = choice as u8;
                // Always positive, but strongly history-dependent so the test
                // checks that encoder and decoder traverse identical model states.
                1 + (self.0 ^ (choice as u64).wrapping_mul(0x9e37_79b9))
                    .rotate_left(choice as u32 & 31)
                    % 10_000
            });
            ByteCategorical::from_weights(weights).unwrap()
        }
    }

    #[derive(Default)]
    struct HistoryModel {
        state: u64,
    }

    impl<T: ?Sized> Model<T> for HistoryModel {
        type Prediction<'a> = HistoryPrediction;

        fn predict(&self) -> Self::Prediction<'_> {
            HistoryPrediction(self.state)
        }

        fn observe(&mut self, choice: u8) {
            self.state = self
                .state
                .wrapping_mul(0x9e37_79b9_7f4a_7c15)
                .wrapping_add(choice as u64 + 1);
        }
    }

    #[derive(Clone)]
    struct TraceStep {
        choice: u8,
        weights: [u64; 256],
    }

    #[derive(Default)]
    struct TraceEncoder {
        steps: Vec<TraceStep>,
    }

    impl ChoiceEncoder for TraceEncoder {
        fn encode(&mut self, choice: u8, distribution: &ByteCategorical) -> Result<()> {
            self.steps.push(TraceStep {
                choice,
                weights: std::array::from_fn(|candidate| distribution.weight(candidate as u8)),
            });
            Ok(())
        }
    }

    struct TraceDecoder<'a> {
        steps: &'a [TraceStep],
        cursor: usize,
    }

    impl ChoiceDecoder for TraceDecoder<'_> {
        fn decode(&mut self, distribution: &ByteCategorical) -> Result<u8> {
            let step = &self.steps[self.cursor];
            self.cursor += 1;
            let weights = std::array::from_fn(|candidate| distribution.weight(candidate as u8));
            assert_eq!(weights, step.weights);
            Ok(step.choice)
        }
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Example {
        flag: bool,
        count: u16,
        name: String,
        maybe: Option<u8>,
        values: Vec<i32>,
        mode: Mode,
    }

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    enum Mode {
        A,
        B(u32),
        C { x: i8, y: bool },
    }

    #[test]
    fn same_model_drives_serialize_and_deserialize() {
        let value = Example {
            flag: true,
            count: 65_000,
            name: "mercy 😄".to_owned(),
            maybe: Some(17),
            values: vec![-1, 0, 1, 123_456],
            mode: Mode::C { x: -7, y: true },
        };

        let mut encode_model = HistoryModel::default();
        let mut encoder = TraceEncoder::default();
        encode(&value, &mut encode_model, &mut encoder).unwrap();

        let mut decode_model = HistoryModel::default();
        let mut decoder = TraceDecoder {
            steps: &encoder.steps,
            cursor: 0,
        };
        let decoded: Example = decode(&mut decode_model, &mut decoder).unwrap();

        assert_eq!(decoded, value);
        assert_eq!(decoder.cursor, encoder.steps.len());
        assert_eq!(decode_model.state, encode_model.state);
    }

    #[test]
    fn byte_categorical_restricts_exactly_to_valid_choices() {
        let weights = std::array::from_fn(|choice| choice as u64 + 1);
        let distribution = ByteCategorical::from_weights(weights)
            .unwrap()
            .restricted(Choices::BINARY)
            .unwrap();
        assert_eq!(distribution.total(), 3);
        assert_eq!(distribution.interval(0), (0, 1));
        assert_eq!(distribution.interval(1), (1, 3));
        assert_eq!(distribution.weight(2), 0);
    }
}
