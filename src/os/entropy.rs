use web_time::{SystemTime, UNIX_EPOCH};

pub fn secret() -> [u8; 8] {
    let mut bytes = platform();
    let clock = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_nanos() as u64)
        .unwrap_or(0);
    for (slot, byte) in bytes.iter_mut().zip(clock.to_le_bytes()) {
        *slot ^= byte;
    }
    bytes
}

#[cfg(not(target_arch = "wasm32"))]
fn platform() -> [u8; 8] {
    use std::io::Read;
    let mut bytes = [0u8; 8];
    if let Ok(mut urandom) = std::fs::File::open("/dev/urandom") {
        let _ = urandom.read_exact(&mut bytes);
    }
    bytes
}

#[cfg(target_arch = "wasm32")]
fn platform() -> [u8; 8] {
    let mut bytes = [0u8; 8];
    for chunk in bytes.chunks_mut(4) {
        let word = (js_sys::Math::random() * 4294967296.0) as u32;
        chunk.copy_from_slice(&word.to_le_bytes()[..chunk.len()]);
    }
    bytes
}
