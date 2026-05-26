use crate::error::Result;
use crate::providers::Embedder;
use async_trait::async_trait;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Deterministic embedder for tests. Output is a function of input text +
/// position only — no network, no randomness, no model dependencies.
#[derive(Debug, Clone)]
pub struct MockEmbedder {
    dim: usize,
}

impl MockEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

#[async_trait]
impl Embedder for MockEmbedder {
    fn dim(&self) -> usize {
        self.dim
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // NOTE: `DefaultHasher` is explicitly **not** stable across Rust versions
        // (per the standard library docs). Vectors produced here may change on a
        // toolchain upgrade. This is fine for unit / integration tests that compare
        // relative distances, but must NOT be used for golden-file tests that
        // snapshot raw float values.
        let mut h = DefaultHasher::new();
        text.hash(&mut h);
        let seed = h.finish();
        // Build a stable pseudo-random vector by combining text hash with position.
        let mut out = Vec::with_capacity(self.dim);
        for i in 0..self.dim {
            let mut hi = DefaultHasher::new();
            seed.hash(&mut hi);
            (i as u64).hash(&mut hi);
            let raw = hi.finish();
            // Map to [-1, 1)
            let v = (raw as f64 / u64::MAX as f64) * 2.0 - 1.0;
            out.push(v as f32);
        }
        Ok(out)
    }
}
