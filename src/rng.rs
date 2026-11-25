//! Seeded random numbers (SplitMix64): the same seed gives the same fight.
//! The game asks its `IRng` two things only: pick one of N options, and wander or idle.

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// `IRng.Pick`: an index in `0..count`.
    pub fn pick(&mut self, count: usize) -> usize {
        if count <= 1 {
            return 0;
        }
        (self.next() % count as u64) as usize
    }

    /// `IRng.WanderRatherThanIdle`: a coin flip.
    pub fn wander_rather_than_idle(&mut self) -> bool {
        self.next() & 1 == 0
    }
}
