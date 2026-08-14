use super::{split, FractionalU16, OutputBytes, FULL_RANGE, RANGE_TOP};

#[inline(always)]
fn append_byte(value: u32, byte: u8) -> u32 {
    let [_, a, b, c] = value.to_be_bytes();
    u32::from_be_bytes([a, b, c, byte])
}

pub struct RangeEncoder {
    low: u32,
    denominator: u32,
    pending: usize,
}

impl Default for RangeEncoder {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeEncoder {
    pub const fn new() -> Self {
        Self {
            low: 0,
            denominator: FULL_RANGE - 1,
            pending: 0,
        }
    }

    #[inline(always)]
    pub fn put(&mut self, p: FractionalU16, lower: bool) -> OutputBytes {
        let mut output = OutputBytes::new();
        let split = split(self.denominator + 1, p);

        debug_assert!(split != 0);
        debug_assert!(split <= self.denominator);

        if lower {
            self.denominator = split - 1;
        } else {
            let [old_head, ..] = self.low.to_be_bytes();

            self.low = self.low.wrapping_add(split);
            self.denominator -= split;
            let [head, ..] = self.low.to_be_bytes();

            if head != old_head {
                if self.pending == 0 {
                    self.pending = 1;
                } else {
                    output.push(head, 0x00, self.pending - 1);
                    let [_, a, b, c] = self.low.to_be_bytes();
                    self.low = u32::from_be_bytes([0, a, b, c]);
                    self.pending = 0;
                }
            }
        }

        self.renormalize(&mut output);
        output
    }

    #[inline(always)]
    fn renormalize(&mut self, output: &mut OutputBytes) {
        for _ in 0..3 {
            if self.denominator < RANGE_TOP {
                let [head, next, a, b] = self.low.to_be_bytes();

                if self.pending == 0 {
                    self.low = u32::from_be_bytes([next, a, b, 0]);
                    self.pending = 1;
                } else if next == 0xff {
                    self.low = u32::from_be_bytes([head, a, b, 0]);
                    self.pending += 1;
                } else {
                    output.push(head, 0xff, self.pending - 1);
                    self.low = u32::from_be_bytes([next, a, b, 0]);
                    self.pending = 1;
                }

                self.denominator = append_byte(self.denominator, 0xff);
            }
        }
    }

    #[inline(always)]
    pub fn finish(mut self) -> OutputBytes {
        let mut output = OutputBytes::new();
        self.denominator = 0;
        self.renormalize(&mut output);

        if self.pending != 0 {
            let [head, ..] = self.low.to_be_bytes();
            output.push(head, 0xff, self.pending - 1);
        }

        output
    }
}

#[derive(Clone, Copy)]
pub struct RangeDecoder<'a> {
    numerator: u32,
    denominator: u32,
    input: &'a [u8],
}

impl<'a> RangeDecoder<'a> {
    #[inline(always)]
    pub fn new(input: &'a [u8]) -> Self {
        let mut decoder = Self {
            numerator: 0,
            denominator: 0,
            input,
        };

        decoder.renormalize();
        decoder
    }

    #[inline(always)]
    pub fn test(&mut self, p: FractionalU16) -> bool {
        let split = split(self.denominator + 1, p);
        debug_assert!(split != 0);
        debug_assert!(split <= self.denominator);

        let lower = self.numerator < split;

        if lower {
            self.denominator = split - 1;
        } else {
            self.numerator -= split;
            self.denominator -= split;
        }

        self.renormalize();
        lower
    }

    #[inline(always)]
    fn renormalize(&mut self) {
        for _ in 0..3 {
            if self.denominator < RANGE_TOP {
                self.denominator = append_byte(self.denominator, 0xff);
                self.numerator = append_byte(self.numerator, self.read_byte());
            }
        }
    }

    #[inline(always)]
    fn read_byte(&mut self) -> u8 {
        let [byte, rest @ ..] = self.input else {
            return 0;
        };

        self.input = rest;
        *byte
    }
}
