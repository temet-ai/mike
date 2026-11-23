mod decode;

pub use decode::{DecodeError, decode_body};

pub const MIN_CHUNK_BYTES: usize = 8 * 1024;
pub const TARGET_CHUNK_BYTES: usize = 16 * 1024;
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;

const BOUNDARY_MASK: u64 = TARGET_CHUNK_BYTES as u64 - 1;
const GEAR: [u64; 256] = gear_table();

pub struct Chunker {
    pending: Vec<u8>,
    rolling: u64,
}

impl Default for Chunker {
    fn default() -> Self {
        Self {
            pending: Vec::with_capacity(MAX_CHUNK_BYTES),
            rolling: 0,
        }
    }
}

impl Chunker {
    pub fn push(&mut self, bytes: &[u8]) -> Vec<Vec<u8>> {
        let mut completed = Vec::new();
        for &byte in bytes {
            self.pending.push(byte);
            self.rolling = self
                .rolling
                .wrapping_shl(1)
                .wrapping_add(GEAR[byte as usize]);
            if self.pending.len() >= MIN_CHUNK_BYTES
                && (self.rolling & BOUNDARY_MASK == 0 || self.pending.len() == MAX_CHUNK_BYTES)
            {
                completed.push(std::mem::replace(
                    &mut self.pending,
                    Vec::with_capacity(MAX_CHUNK_BYTES),
                ));
                self.rolling = 0;
            }
        }
        completed
    }

    pub fn finish(self) -> Option<Vec<u8>> {
        (!self.pending.is_empty()).then_some(self.pending)
    }
}

pub struct KnownChunk<'a> {
    pub id: i64,
    pub bytes: &'a [u8],
}

pub fn matching_chunk_id(bytes: &[u8], candidates: &[KnownChunk<'_>]) -> Option<i64> {
    candidates
        .iter()
        .find(|candidate| candidate.bytes == bytes)
        .map(|candidate| candidate.id)
}

const fn gear_table() -> [u64; 256] {
    let mut table = [0; 256];
    let mut index = 0;
    while index < table.len() {
        let mut value = (index as u64).wrapping_add(0x9e37_79b9_7f4a_7c15);
        value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        table[index] = value ^ (value >> 31);
        index += 1;
    }
    table
}

#[cfg(test)]
mod tests;
