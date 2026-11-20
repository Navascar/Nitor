//! Генератор випадкових чисел, сумісний з `random.Random` Python (MT19937).
//!
//! Ефект «Випадкові кольори» має давати ті самі кольори, що й Nitor 1.x, — це перевіряють
//! золоті тести. Реалізовано лише те, що потрібно: `Random(seed)`, `random()`, `choice()`.

const N: usize = 624;
const M: usize = 397;

pub struct PyRandom {
    mt: [u32; N],
    index: usize,
}

impl PyRandom {
    /// `random.Random(seed)` для цілого seed.
    pub fn new(seed: u64) -> Self {
        let mut key: Vec<u32> = Vec::new();
        let mut n = seed;
        while n > 0 {
            key.push(n as u32);
            n >>= 32;
        }
        if key.is_empty() {
            key.push(0);
        }
        let mut r = PyRandom { mt: [0; N], index: N };
        r.init_by_array(&key);
        r
    }

    fn init_genrand(&mut self, s: u32) {
        self.mt[0] = s;
        for i in 1..N {
            let prev = self.mt[i - 1];
            self.mt[i] = 1812433253u32.wrapping_mul(prev ^ (prev >> 30)).wrapping_add(i as u32);
        }
        self.index = N;
    }

    fn init_by_array(&mut self, key: &[u32]) {
        self.init_genrand(19650218);
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..N.max(key.len()) {
            let prev = self.mt[i - 1];
            self.mt[i] =
                (self.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1664525)).wrapping_add(key[j]).wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            let prev = self.mt[i - 1];
            self.mt[i] = (self.mt[i] ^ (prev ^ (prev >> 30)).wrapping_mul(1566083941)).wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                self.mt[0] = self.mt[N - 1];
                i = 1;
            }
        }
        self.mt[0] = 0x8000_0000;
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            for k in 0..N {
                let y = (self.mt[k] & 0x8000_0000) | (self.mt[(k + 1) % N] & 0x7fff_ffff);
                let mut v = self.mt[(k + M) % N] ^ (y >> 1);
                if y & 1 != 0 {
                    v ^= 0x9908_b0df;
                }
                self.mt[k] = v;
            }
            self.index = 0;
        }
        let mut y = self.mt[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^= y >> 18;
        y
    }

    /// `random()` — число з [0, 1) з 53 бітами точності.
    pub fn random(&mut self) -> f64 {
        let a = (self.next_u32() >> 5) as f64;
        let b = (self.next_u32() >> 6) as f64;
        (a * 67108864.0 + b) * (1.0 / 9007199254740992.0)
    }

    fn getrandbits(&mut self, k: u32) -> u32 {
        debug_assert!((1..=32).contains(&k));
        self.next_u32() >> (32 - k)
    }

    /// `_randbelow(n)` — рівномірно з 0..n.
    pub fn below(&mut self, n: usize) -> usize {
        let k = usize::BITS - n.leading_zeros();
        loop {
            let r = self.getrandbits(k) as usize;
            if r < n {
                return r;
            }
        }
    }

    /// `choice(seq)`.
    pub fn choice<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_python() {
        // python3 -c "import random; r=random.Random(42); print(r.random(), r.random())"
        let mut r = PyRandom::new(42);
        assert_eq!(r.random(), 0.6394267984578837);
        assert_eq!(r.random(), 0.025010755222666936);
        // python3 -c "import random; print(random.Random(0).random())"
        assert_eq!(PyRandom::new(0).random(), 0.8444218515250481);
        // python3 -c "import random; print(random.Random(2**40+5).random())"
        assert_eq!(PyRandom::new((1 << 40) + 5).random(), PY_BIG);
    }

    const PY_BIG: f64 = 0.5043802970418443;

    #[test]
    fn choice_matches_python() {
        // python3 -c "import random; r=random.Random(7); print([r.choice([1,2,3,4,5]) for _ in range(8)])"
        let mut r = PyRandom::new(7);
        let items = [1, 2, 3, 4, 5];
        let got: Vec<i32> = (0..8).map(|_| *r.choice(&items)).collect();
        assert_eq!(got, [3, 2, 4, 1, 1, 5, 1, 3]);
    }
}
