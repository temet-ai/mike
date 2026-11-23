use super::*;

fn chunked(bytes: &[u8], feed_size: usize) -> Vec<Vec<u8>> {
    let mut chunker = Chunker::default();
    let mut chunks = Vec::new();
    for bytes in bytes.chunks(feed_size) {
        chunks.extend(chunker.push(bytes));
        assert!(chunker.pending.len() < MAX_CHUNK_BYTES);
        assert!(chunker.pending.capacity() <= MAX_CHUNK_BYTES);
    }
    chunks.extend(chunker.finish());
    chunks
}

fn fixture(size: usize) -> Vec<u8> {
    let mut state = 0x6a09_e667_f3bc_c909_u64;
    (0..size)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as u8
        })
        .collect()
}

fn reused_bytes(chunks: &[Vec<u8>], prior: &[Vec<u8>]) -> usize {
    let known: Vec<_> = prior
        .iter()
        .enumerate()
        .map(|(id, bytes)| KnownChunk {
            id: id as i64,
            bytes,
        })
        .collect();
    chunks
        .iter()
        .filter(|chunk| matching_chunk_id(chunk, &known).is_some())
        .map(Vec::len)
        .sum()
}

#[test]
fn binary_input_arbitrary_feed_boundaries_reconstructs_every_byte() {
    let bytes = fixture(512 * 1024 + 7);
    let reference = chunked(&bytes, bytes.len());
    for feed_size in [1, 7, 8191, MIN_CHUNK_BYTES, 65537] {
        let chunks = chunked(&bytes, feed_size);
        assert_eq!(chunks, reference);
        assert_eq!(chunks.concat(), bytes);
    }
}

#[test]
fn large_input_working_buffer_stays_bounded_without_losing_bytes() {
    let bytes = fixture(8 * 1024 * 1024 + 3);
    let chunks = chunked(&bytes, 4093);
    assert_eq!(chunks.concat(), bytes);
    assert!(chunks.iter().all(|chunk| chunk.len() <= MAX_CHUNK_BYTES));
    assert!(
        chunks[..chunks.len() - 1]
            .iter()
            .all(|chunk| chunk.len() >= MIN_CHUNK_BYTES)
    );
    let repeated = vec![0; MAX_CHUNK_BYTES * 4 + 1];
    assert_eq!(chunked(&repeated, 19).concat(), repeated);
}

#[test]
fn prefix_insertion_and_append_edits_reuse_unchanged_chunks() {
    let bytes = fixture(2 * 1024 * 1024);
    let prior = chunked(&bytes, 4011);
    let mut prefix = b"new prefix".to_vec();
    prefix.extend_from_slice(&bytes);
    let mut insertion = bytes.clone();
    insertion.splice(91_313..91_313, b"inserted bytes".iter().copied());
    let mut append = bytes.clone();
    append.extend_from_slice(b"new trailing bytes");
    for changed in [prefix, insertion, append] {
        let chunks = chunked(&changed, 137);
        assert_eq!(chunks.concat(), changed);
        assert!(reused_bytes(&chunks, &prior) > bytes.len() * 9 / 10);
    }
}

#[test]
fn duplicate_context_reuses_supplied_ids_and_does_not_invent_matches() {
    let bytes = fixture(1024 * 1024);
    let prior = chunked(&bytes, 2048);
    let again = chunked(&bytes, 17);
    assert_eq!(reused_bytes(&again, &prior), bytes.len());
    let candidates = [
        KnownChunk {
            id: 9,
            bytes: b"different",
        },
        KnownChunk {
            id: 42,
            bytes: b"same",
        },
        KnownChunk {
            id: 43,
            bytes: b"same",
        },
    ];
    assert_eq!(matching_chunk_id(b"same", &candidates), Some(42));
    assert_eq!(matching_chunk_id(b"Same", &candidates), None);
    assert_eq!(matching_chunk_id(b"same", &[]), None);
}

#[test]
fn empty_and_non_utf8_bodies_preserve_exact_payloads() {
    assert!(chunked(&[], 1).is_empty());
    assert!(Chunker::default().finish().is_none());
    for bytes in [b"\0\xff\xfe\x80\0".as_slice(), b"\x28\xb5\x2f\xfd\xff\0"] {
        assert_eq!(chunked(bytes, 1).concat(), bytes);
    }
}

#[test]
fn growing_requests_changed_envelope_reuses_prior_context() -> anyhow::Result<()> {
    let content = fixture(1024 * 1024);
    let alphabet = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789 ";
    let text: String = content
        .iter()
        .map(|byte| alphabet[usize::from(*byte) % alphabet.len()] as char)
        .collect();
    let mut input: Vec<_> = text
        .as_bytes()
        .chunks(4096)
        .map(|bytes| serde_json::json!({
            "role":"user", "content":[{"type":"input_text", "text":String::from_utf8_lossy(bytes)}]
        }))
        .collect();
    let original = serde_json::to_vec(&serde_json::json!({
        "model":"gpt-6.1-sol", "input":input, "stream":true,
        "client_metadata":{"turn_id":"turn-1"}, "unknown_field":{"preserve":true}
    }))?;
    let prior = chunked(&original, 4096);
    input.push(
        serde_json::json!({"type":"function_call_output", "call_id":"call-1",
        "output":"a new tool result"}),
    );
    let grown = serde_json::to_vec(&serde_json::json!({
        "model":"gpt-6.2-sol", "input":input, "stream":true,
        "client_metadata":{"turn_id":"turn-200"}, "unknown_field":{"preserve":true}
    }))?;
    let chunks = chunked(&grown, 1777);
    assert_eq!(chunks.concat(), grown);
    let reused = reused_bytes(&chunks, &prior);
    assert!(reused > original.len() * 9 / 10);
    eprintln!(
        "growing request: {} original bytes, {} current bytes, {reused} reused bytes",
        original.len(),
        grown.len()
    );
    Ok(())
}
