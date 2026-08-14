use std::marker::PhantomData;

use super::{FractionalU16, OutputBytes, FULL_RANGE, RANGE_TOP};

pub trait Grid {
    type Probability: Copy;

    fn split(range: u32, p: Self::Probability) -> u32;
}

pub struct BaselineEncoder<G: Grid> {
    low: u32,
    denominator: u32,
    pending: usize,
    grid: PhantomData<fn() -> G>,
}

impl<G: Grid> Default for BaselineEncoder<G> {
    fn default() -> Self {
        Self::new()
    }
}

impl<G: Grid> BaselineEncoder<G> {
    pub const fn new() -> Self {
        Self {
            low: 0,
            denominator: FULL_RANGE - 1,
            pending: 0,
            grid: PhantomData,
        }
    }

    pub fn put(&mut self, p: G::Probability, lower: bool) -> OutputBytes {
        let mut output = OutputBytes::new();
        let split = G::split(self.denominator + 1, p);

        debug_assert!(split != 0);
        debug_assert!(split <= self.denominator);

        if lower {
            self.denominator = split - 1;
        } else {
            let old_head = self.low >> 24;

            self.low = self.low.wrapping_add(split);
            self.denominator -= split;
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

    fn renormalize(&mut self, output: &mut OutputBytes) {
        for _ in 0..3 {
            if self.denominator < RANGE_TOP {
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

                self.denominator = (self.denominator << 8) | 0xff;
            }
        }
    }

    pub fn finish(mut self) -> OutputBytes {
        let mut output = OutputBytes::new();
        self.denominator = 0;
        self.renormalize(&mut output);

        if self.pending != 0 {
            output.push((self.low >> 24) as u8, 0xff, self.pending - 1);
        }

        output
    }
}

pub struct BaselineDecoder<'a, G: Grid> {
    numerator: u32,
    denominator: u32,
    input: &'a [u8],
    grid: PhantomData<fn() -> G>,
}

impl<'a, G: Grid> Clone for BaselineDecoder<'a, G> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<'a, G: Grid> Copy for BaselineDecoder<'a, G> {}

impl<'a, G: Grid> BaselineDecoder<'a, G> {
    pub fn new(input: &'a [u8]) -> Self {
        let mut decoder = Self {
            numerator: 0,
            denominator: 0,
            input,
            grid: PhantomData,
        };

        decoder.renormalize();
        decoder
    }

    pub fn test(&mut self, p: G::Probability) -> bool {
        let split = G::split(self.denominator + 1, p);
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

    fn renormalize(&mut self) {
        for _ in 0..3 {
            if self.denominator < RANGE_TOP {
                self.denominator = (self.denominator << 8) | 0xff;
                self.numerator = (self.numerator << 8) | self.read_byte() as u32;
            }
        }
    }

    fn read_byte(&mut self) -> u8 {
        let [byte, rest @ ..] = self.input else {
            return 0;
        };

        self.input = rest;
        *byte
    }
}

pub mod q16 {
    use super::{BaselineDecoder, BaselineEncoder, Grid};

    #[repr(transparent)]
    #[derive(Clone, Copy)]
    pub struct Probability(u16);

    impl Probability {
        pub const fn from_raw(raw: u16) -> Self {
            Self(raw)
        }
    }

    pub struct Q16;

    impl Grid for Q16 {
        type Probability = Probability;

        #[inline(always)]
        fn split(range: u32, p: Probability) -> u32 {
            debug_assert!(p.0 >= 0x8000);
            ((range as u64 * p.0 as u64) >> 16) as u32
        }
    }

    pub type RangeEncoder = BaselineEncoder<Q16>;
    pub type RangeDecoder<'a> = BaselineDecoder<'a, Q16>;
}

pub mod q17 {
    use super::{super::split, BaselineDecoder, BaselineEncoder, FractionalU16, Grid};

    pub struct Q17;

    impl Grid for Q17 {
        type Probability = FractionalU16;

        #[inline(always)]
        fn split(range: u32, p: FractionalU16) -> u32 {
            split(range, p)
        }
    }

    pub type RangeEncoder = BaselineEncoder<Q17>;
    pub type RangeDecoder<'a> = BaselineDecoder<'a, Q17>;
}

pub mod range_shift {
    use super::{super::split, FractionalU16, OutputBytes, FULL_RANGE, RANGE_TOP};

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
        pub fn new(input: &'a [u8]) -> Self {
            let mut decoder = Self {
                numerator: 0,
                range: 1,
                input,
            };

            decoder.renormalize();
            decoder
        }

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

        fn renormalize(&mut self) {
            for _ in 0..3 {
                if self.range <= RANGE_TOP {
                    self.range <<= 8;
                    self.numerator = (self.numerator << 8) | self.read_byte() as u32;
                }
            }
        }

        fn read_byte(&mut self) -> u8 {
            let [byte, rest @ ..] = self.input else {
                return 0;
            };

            self.input = rest;
            *byte
        }
    }
}

pub mod branchless {
    use std::hint::select_unpredictable;

    use super::{super::split, FractionalU16, OutputBytes, FULL_RANGE, RANGE_TOP};

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

        pub fn put(&mut self, p: FractionalU16, lower: bool) -> OutputBytes {
            let mut output = OutputBytes::new();
            let split = split(self.range, p);

            debug_assert!(split != 0);
            debug_assert!(split < self.range);

            let old_head = self.low >> 24;
            let offset = select_unpredictable(lower, 0, split);
            self.low = self.low.wrapping_add(offset);
            self.range = select_unpredictable(lower, split, self.range - split);
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

            self.renormalize(&mut output);
            output
        }

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
        pub fn new(input: &'a [u8]) -> Self {
            let mut decoder = Self {
                numerator: 0,
                range: 1,
                input,
            };

            decoder.renormalize();
            decoder
        }

        pub fn test(&mut self, p: FractionalU16) -> bool {
            let split = split(self.range, p);
            debug_assert!(split != 0);
            debug_assert!(split < self.range);

            let lower = self.numerator < split;
            let offset = select_unpredictable(lower, 0, split);

            self.numerator = self.numerator.wrapping_sub(offset);
            self.range = select_unpredictable(lower, split, self.range - split);

            self.renormalize();
            lower
        }

        fn renormalize(&mut self) {
            for _ in 0..3 {
                if self.range <= RANGE_TOP {
                    self.range <<= 8;
                    self.numerator = (self.numerator << 8) | self.read_byte() as u32;
                }
            }
        }

        fn read_byte(&mut self) -> u8 {
            let [byte, rest @ ..] = self.input else {
                return 0;
            };

            self.input = rest;
            *byte
        }
    }
}
