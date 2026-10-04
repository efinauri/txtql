//! Small shared helpers.

use crate::error::MatchError;
use std::collections::{HashMap, HashSet};
use std::hash::{BuildHasherDefault, Hasher};

/// The Fx hash (as used in rustc): fast and good enough for integer keys.
#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

const SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut buf = [0u8; 8];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.add(u64::from_le_bytes(buf));
        }
    }

    #[inline]
    fn write_u32(&mut self, i: u32) {
        self.add(i.into());
    }

    #[inline]
    fn write_u64(&mut self, i: u64) {
        self.add(i);
    }

    #[inline]
    fn write_usize(&mut self, i: usize) {
        self.add(i as u64);
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }
}

pub type FxBuild = BuildHasherDefault<FxHasher>;
pub type FxHashMap<K, V> = HashMap<K, V, FxBuild>;
pub type FxHashSet<K> = HashSet<K, FxBuild>;

/// Shared step budget for recognition, extraction and ambiguity analysis.
#[derive(Debug, Clone)]
pub struct Budget {
    pub limit: u64,
    pub used: u64,
}

impl Budget {
    pub fn new(limit: u64) -> Budget {
        Budget { limit, used: 0 }
    }

    #[inline]
    pub fn spend(&mut self, n: u64) -> Result<(), MatchError> {
        self.used += n;
        if self.used > self.limit { Err(MatchError::TooExpensive { limit: self.limit }) } else { Ok(()) }
    }
}

/// Runs `f`, first moving to a fresh heap-allocated stack segment if less than 128 KiB of the
/// current stack is left. Every recursive path (tree extraction, value building, templates)
/// goes through this, so deep nesting cannot overflow the thread's stack; `max_depth` only
/// bounds memory.
#[inline]
pub fn with_stack<R>(f: impl FnOnce() -> R) -> R {
    stacker::maybe_grow(128 * 1024, 4 * 1024 * 1024, f)
}
