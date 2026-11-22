use std::path::Path;

pub(super) fn digest_dir(dir: &Path) -> std::io::Result<String> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let bytes = std::fs::read(entry.path())?;
        files.push((name, bytes));
    }
    Ok(digest_files(&files))
}

pub(super) fn digest_files(files: &[(String, Vec<u8>)]) -> String {
    let mut sorted: Vec<&(String, Vec<u8>)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut hash = Fnv::new();
    hash.write(&(sorted.len() as u64).to_le_bytes());
    for (name, bytes) in sorted {
        hash.write(&(name.len() as u64).to_le_bytes());
        hash.write(name.as_bytes());
        hash.write(&(bytes.len() as u64).to_le_bytes());
        hash.write(bytes);
    }
    format!("{:016x}", hash.0)
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
}

#[cfg(test)]
#[path = "migrations_digest_tests.rs"]
mod migrations_digest_tests;
