//! The story PRNG: xoroshiro128++ seeded by two SplitMix64 outputs (`bytecode.md` §PRNG).

/// xoroshiro128++ state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rng {
    pub(crate) s0: u64,
    pub(crate) s1: u64,
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ z.wrapping_shr(30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ z.wrapping_shr(27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ z.wrapping_shr(31)
}

impl Rng {
    /// `s0` is the first SplitMix64 output of `seed`, `s1` the second.
    pub(crate) fn from_seed(seed: u64) -> Rng {
        let mut state = seed;
        let s0 = splitmix64(&mut state);
        let s1 = splitmix64(&mut state);
        Rng { s0, s1 }
    }

    pub(crate) fn next_u64(&mut self) -> u64 {
        let (s0, mut s1) = (self.s0, self.s1);
        let result = s0.wrapping_add(s1).rotate_left(17).wrapping_add(s0);
        s1 ^= s0;
        self.s0 = s0.rotate_left(49) ^ s1 ^ s1.wrapping_shl(21);
        self.s1 = s1.rotate_left(28);
        result
    }

    /// A uniform draw from `0..n`, for `1 ≤ n ≤ 2^64`; `None` outside that range.
    pub(crate) fn uniform(&mut self, n: u128) -> Option<u64> {
        if n == 1u128 << 64 {
            return Some(self.next_u64());
        }
        let n = u64::try_from(n).ok().filter(|n| *n > 0)?;
        let t = n.wrapping_neg().checked_rem(n)?;
        loop {
            let r = self.next_u64();
            if r >= t {
                return r.checked_rem(n);
            }
        }
    }
}
