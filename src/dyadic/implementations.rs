//! Alternate storage layouts for exact dyadic arithmetic benchmarks.

use bitvec::prelude::{BitBox, BitVec, Lsb0};
use num_bigint::BigUint;

use crate::BigDyadic;

#[derive(Clone, Copy, Debug)]
pub struct Candidate {
    pub name: &'static str,
    prepare: fn(&BigDyadic) -> Value,
    multiply: fn(&Value, &Value) -> Value,
    scale_floor: fn(&Value, u64) -> u64,
}

impl Candidate {
    const fn new(
        name: &'static str,
        prepare: fn(&BigDyadic) -> Value,
        multiply: fn(&Value, &Value) -> Value,
        scale_floor: fn(&Value, u64) -> u64,
    ) -> Self {
        Self {
            name,
            prepare,
            multiply,
            scale_floor,
        }
    }

    pub fn prepare(self, value: &BigDyadic) -> Value {
        (self.prepare)(value)
    }

    pub fn multiply(self, left: &Value, right: &Value) -> Value {
        (self.multiply)(left, right)
    }

    pub fn scale_floor_u64(self, value: &Value, scale: u64) -> u64 {
        (self.scale_floor)(value, scale)
    }
}

#[derive(Clone, Debug)]
pub struct Value(Representation);

#[derive(Clone, Debug)]
enum Representation {
    BitBoxMsb(BigDyadic),
    BitBoxLsb {
        bits: BitBox<u8, Lsb0>,
    },
    BigEndianBytes {
        numerator: Box<[u8]>,
        fractional_bits: usize,
    },
    LittleEndianBytes {
        numerator: Box<[u8]>,
        fractional_bits: usize,
    },
    NativeBigUint {
        numerator: BigUint,
        fractional_bits: usize,
    },
}

impl Value {
    pub fn to_big_dyadic(&self) -> BigDyadic {
        let (numerator, fractional_bits) = parts(&self.0);
        BigDyadic::from_scaled(numerator, fractional_bits)
    }
}

pub const BITBOX_MSB: Candidate =
    Candidate::new("bitbox-msb", prepare_msb, multiply_msb, scale_msb);
pub const BITBOX_LSB: Candidate =
    Candidate::new("bitbox-lsb", prepare_lsb, multiply_lsb, scale_lsb);
pub const BIG_ENDIAN_BYTES: Candidate = Candidate::new(
    "big-endian-bytes",
    prepare_big_endian,
    multiply_big_endian,
    scale_big_endian,
);
pub const LITTLE_ENDIAN_BYTES: Candidate = Candidate::new(
    "little-endian-bytes",
    prepare_little_endian,
    multiply_little_endian,
    scale_little_endian,
);
pub const NATIVE_BIGUINT: Candidate = Candidate::new(
    "native-biguint",
    prepare_native,
    multiply_native,
    scale_native,
);

pub const CANDIDATES: &[Candidate] = &[
    BITBOX_MSB,
    BITBOX_LSB,
    BIG_ENDIAN_BYTES,
    LITTLE_ENDIAN_BYTES,
    NATIVE_BIGUINT,
];

/// Fastest arithmetic layout across the durable multiplication and scaling matrix.
pub const DEFAULT: Candidate = NATIVE_BIGUINT;

fn canonicalize(mut numerator: BigUint, mut fractional_bits: usize) -> (BigUint, usize) {
    if numerator.bits() == 0 {
        return (numerator, 0);
    }
    let removable = numerator
        .trailing_zeros()
        .unwrap_or(0)
        .min(fractional_bits as u64) as usize;
    numerator >>= removable;
    fractional_bits -= removable;
    (numerator, fractional_bits)
}

fn parts(value: &Representation) -> (BigUint, usize) {
    match value {
        Representation::BitBoxMsb(value) => (value.numerator(), value.fractional_bits()),
        Representation::BitBoxLsb { bits } => {
            let mut numerator = BigUint::from(0u8);
            for (bit, set) in bits.iter().by_vals().rev().enumerate() {
                if set {
                    numerator.set_bit(bit as u64, true);
                }
            }
            (numerator, bits.len() - 1)
        }
        Representation::BigEndianBytes {
            numerator,
            fractional_bits,
        } => (BigUint::from_bytes_be(numerator), *fractional_bits),
        Representation::LittleEndianBytes {
            numerator,
            fractional_bits,
        } => (BigUint::from_bytes_le(numerator), *fractional_bits),
        Representation::NativeBigUint {
            numerator,
            fractional_bits,
        } => (numerator.clone(), *fractional_bits),
    }
}

fn lsb_value(numerator: BigUint, fractional_bits: usize) -> Value {
    let (numerator, fractional_bits) = canonicalize(numerator, fractional_bits);
    let width = fractional_bits + 1;
    let mut bits = BitVec::<u8, Lsb0>::repeat(false, width);
    for bit in 0..numerator.bits() as usize {
        if numerator.bit(bit as u64) {
            bits.set(width - 1 - bit, true);
        }
    }
    Value(Representation::BitBoxLsb {
        bits: bits.into_boxed_bitslice(),
    })
}

fn big_endian_value(numerator: BigUint, fractional_bits: usize) -> Value {
    let (numerator, fractional_bits) = canonicalize(numerator, fractional_bits);
    Value(Representation::BigEndianBytes {
        numerator: numerator.to_bytes_be().into_boxed_slice(),
        fractional_bits,
    })
}

fn little_endian_value(numerator: BigUint, fractional_bits: usize) -> Value {
    let (numerator, fractional_bits) = canonicalize(numerator, fractional_bits);
    Value(Representation::LittleEndianBytes {
        numerator: numerator.to_bytes_le().into_boxed_slice(),
        fractional_bits,
    })
}

fn native_value(numerator: BigUint, fractional_bits: usize) -> Value {
    let (numerator, fractional_bits) = canonicalize(numerator, fractional_bits);
    Value(Representation::NativeBigUint {
        numerator,
        fractional_bits,
    })
}

fn prepare_msb(value: &BigDyadic) -> Value {
    Value(Representation::BitBoxMsb(value.clone()))
}

fn prepare_lsb(value: &BigDyadic) -> Value {
    lsb_value(value.numerator(), value.fractional_bits())
}

fn prepare_big_endian(value: &BigDyadic) -> Value {
    big_endian_value(value.numerator(), value.fractional_bits())
}

fn prepare_little_endian(value: &BigDyadic) -> Value {
    little_endian_value(value.numerator(), value.fractional_bits())
}

fn prepare_native(value: &BigDyadic) -> Value {
    native_value(value.numerator(), value.fractional_bits())
}

fn multiply_parts(left: &Representation, right: &Representation) -> (BigUint, usize) {
    let (left_numerator, left_bits) = parts(left);
    let (right_numerator, right_bits) = parts(right);
    (
        left_numerator * right_numerator,
        left_bits
            .checked_add(right_bits)
            .expect("dyadic precision overflow"),
    )
}

fn multiply_msb(left: &Value, right: &Value) -> Value {
    let (Representation::BitBoxMsb(left), Representation::BitBoxMsb(right)) = (&left.0, &right.0)
    else {
        panic!("layout mismatch")
    };
    Value(Representation::BitBoxMsb(left.multiplied(right)))
}

fn multiply_lsb(left: &Value, right: &Value) -> Value {
    let (numerator, bits) = multiply_parts(&left.0, &right.0);
    lsb_value(numerator, bits)
}

fn multiply_big_endian(left: &Value, right: &Value) -> Value {
    let (numerator, bits) = multiply_parts(&left.0, &right.0);
    big_endian_value(numerator, bits)
}

fn multiply_little_endian(left: &Value, right: &Value) -> Value {
    let (numerator, bits) = multiply_parts(&left.0, &right.0);
    little_endian_value(numerator, bits)
}

fn multiply_native(left: &Value, right: &Value) -> Value {
    let (
        Representation::NativeBigUint {
            numerator: left,
            fractional_bits: left_bits,
        },
        Representation::NativeBigUint {
            numerator: right,
            fractional_bits: right_bits,
        },
    ) = (&left.0, &right.0)
    else {
        panic!("layout mismatch")
    };
    native_value(
        left * right,
        left_bits
            .checked_add(*right_bits)
            .expect("dyadic precision overflow"),
    )
}

fn scale_parts(value: &Representation, scale: u64) -> u64 {
    let (numerator, fractional_bits) = parts(value);
    let digits = ((numerator * BigUint::from(scale)) >> fractional_bits).to_u64_digits();
    match digits.as_slice() {
        [] => 0,
        [scaled] => *scaled,
        _ => unreachable!("scaled probability cannot exceed input"),
    }
}

fn scale_msb(value: &Value, scale: u64) -> u64 {
    let Representation::BitBoxMsb(value) = &value.0 else {
        panic!("layout mismatch")
    };
    value.scale_floor_u64(scale)
}

fn scale_lsb(value: &Value, scale: u64) -> u64 {
    scale_parts(&value.0, scale)
}

fn scale_big_endian(value: &Value, scale: u64) -> u64 {
    scale_parts(&value.0, scale)
}

fn scale_little_endian(value: &Value, scale: u64) -> u64 {
    scale_parts(&value.0, scale)
}

fn scale_native(value: &Value, scale: u64) -> u64 {
    let Representation::NativeBigUint {
        numerator,
        fractional_bits,
    } = &value.0
    else {
        panic!("layout mismatch")
    };
    let digits = ((numerator * BigUint::from(scale)) >> fractional_bits).to_u64_digits();
    match digits.as_slice() {
        [] => 0,
        [scaled] => *scaled,
        _ => unreachable!("scaled probability cannot exceed input"),
    }
}
