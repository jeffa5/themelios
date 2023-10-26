use std::hash::Hasher;

const OFFSET_BASIS_32: u32 = 2166136261;

const FNV_PRIME_32: u32 = 16777619;

/// An FNV-1a hasher suitable to match kubernetes'
pub struct FnvHasher(u32);

impl FnvHasher {
    pub fn new_32a() -> Self {
        FnvHasher(OFFSET_BASIS_32)
    }

    pub fn finish_32(&self) -> u32{
        self.0
    }

    pub fn write(&mut self, bytes: &[u8]) {
        let mut hash = self.0;
        for byte in bytes {
            hash ^= *byte as u32;
            hash = hash.wrapping_mul(FNV_PRIME_32);
        }
        self.0 = hash;
    }
}

impl Hasher for FnvHasher{
    fn finish(&self) -> u64 {
        self.0.into()
    }

    fn write(&mut self, bytes: &[u8]) {
        self.write(bytes)
    }
}
