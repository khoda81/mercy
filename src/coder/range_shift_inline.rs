use super::{split, FractionalU16, OutputBytes, FULL_RANGE, RANGE_TOP};

pub struct RangeEncoder {
    low: u32,
    range: u32,
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
            range: FULL_RANGE,
            pending: 0,
        }
    }

    #[inline(always)]
    pub fn put(&mut self, p: FractionalU16, lower: bool) -> OutputBytes {
        let mut output = OutputBytes::new();
        let split = split(self.range, p);

        debug_assert!(split != 0);
        debug_assert!(split < self.range);

        if lower {
            self.range = split;
        } else {
            let old_head = self.low >> 24;

            self.low = self.low.wrapping_add(split);
            self.range -= split;
            let head = self.low >> 24;

            if head != old_head {
                if self.pending == 0 {
                    self.pending = 1;
                } else {
                    output.push(head as u8, 0x00, self.pending - 1);
                    self.low &= 0x00ff_ffff;
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
            if self.range <= RANGE_TOP {
                let head = (self.low >> 24) as u8;
                let next = (self.low >> 16) as u8;

                if self.pending == 0 {
                    self.low <<= 8;
                    self.pending = 1;
                } else if next == 0xff {
                    self.low = (self.low & 0xff00_0000) | ((self.low << 8) & 0x00ff_ff00);
                    self.pending += 1;
                } else {
                    output.push(head, 0xff, self.pending - 1);
                    self.low <<= 8;
                    self.pending = 1;
                }

                self.range <<= 8;
            }
        }
    }

    #[inline(always)]
    pub fn finish(mut self) -> OutputBytes {
        let mut output = OutputBytes::new();
        self.range = 1;
        self.renormalize(&mut output);

        if self.pending != 0 {
            output.push((self.low >> 24) as u8, 0xff, self.pending - 1);
        }

        output
    }
}

#[derive(Clone, Copy)]
pub struct RangeDecoder<'a> {
    numerator: u32,
    range: u32,
    input: &'a [u8],
}

impl<'a> RangeDecoder<'a> {
    #[inline(always)]
    pub fn new(input: &'a [u8]) -> Self {
        let mut decoder = Self {
            numerator: 0,
            range: 1,
            input,
        };

        decoder.renormalize();
        decoder
    }

    #[inline(always)]
    pub fn test(&mut self, p: FractionalU16) -> bool {
        let split = split(self.range, p);
        debug_assert!(split != 0);
        debug_assert!(split < self.range);

        let lower = self.numerator < split;

        if lower {
            self.range = split;
        } else {
            self.numerator -= split;
            self.range -= split;
        }

        self.renormalize();
        lower
    }

    #[inline(always)]
    fn renormalize(&mut self) {
        for _ in 0..3 {
            if self.range <= RANGE_TOP {
                self.range <<= 8;
                self.numerator = (self.numerator << 8) | self.read_byte() as u32;
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
