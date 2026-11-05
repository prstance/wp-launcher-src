//! Nintendo's LZ77, the .lz files of the disc (the RELs, most archives).

/// Nintendo's LZ77 (types 0x10 and 0x11), as the disc's .lz files use.
pub fn decompress(data: &[u8]) -> Result<Vec<u8>, String> {
    let broken = || "LZ: données tronquées".to_string();
    let kind = *data.first().ok_or_else(broken)?;
    if kind != 0x10 && kind != 0x11 {
        return Err(format!("LZ: type {kind:#x} inconnu"));
    }
    let mut size = u32::from_le_bytes([data[1], data[2], data[3], 0]) as usize;
    let mut at = 4;
    if size == 0 {
        size = u32::from_le_bytes(data.get(4..8).ok_or_else(broken)?.try_into().unwrap()) as usize;
        at = 8;
    }
    let mut out = Vec::with_capacity(size);
    let mut next = || -> Result<usize, String> {
        let byte = *data.get(at).ok_or_else(broken)?;
        at += 1;
        Ok(byte as usize)
    };
    while out.len() < size {
        let flags = next()?;
        for bit in (0..8).rev() {
            if out.len() >= size {
                break;
            }
            if flags & (1 << bit) == 0 {
                out.push(next()? as u8);
                continue;
            }
            let b1 = next()?;
            let (len, disp) = if kind == 0x10 {
                let b2 = next()?;
                ((b1 >> 4) + 3, ((b1 & 0xF) << 8 | b2) + 1)
            } else {
                match b1 >> 4 {
                    0 => {
                        let (b2, b3) = (next()?, next()?);
                        (((b1 & 0xF) << 4 | b2 >> 4) + 0x11, ((b2 & 0xF) << 8 | b3) + 1)
                    }
                    1 => {
                        let (b2, b3, b4) = (next()?, next()?, next()?);
                        (((b1 & 0xF) << 12 | b2 << 4 | b3 >> 4) + 0x111, ((b3 & 0xF) << 8 | b4) + 1)
                    }
                    n => {
                        let b2 = next()?;
                        (n + 1, ((b1 & 0xF) << 8 | b2) + 1)
                    }
                }
            };
            let start = out.len().checked_sub(disp).ok_or_else(broken)?;
            for i in 0..len {
                out.push(out[start + i]);
            }
        }
    }
    out.truncate(size);
    Ok(out)
}
/// Window and lengths of type 0x11.
const WINDOW: usize = 0x1000;
const MAX_LEN: usize = 0x10110;
/// Candidates looked at for each position (more: a little smaller, slower).
const CHAIN: usize = 128;

/// Type 0x11, as the game's own files. Greedy, with a hash of the next 3 bytes.
pub fn compress(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() / 2 + 16);
    out.push(0x11);
    // (a size of 0 in the short header means that a 32-bit one follows)
    if (1..=0xFF_FFFF).contains(&data.len()) {
        out.extend_from_slice(&(data.len() as u32).to_le_bytes()[..3]);
    } else {
        out.extend_from_slice(&[0, 0, 0]);
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    }
    let hash = |at: usize| (u32::from(data[at]) << 16 | u32::from(data[at + 1]) << 8 | u32::from(data[at + 2])).wrapping_mul(0x9E37_79B1) >> 17;
    let mut head = vec![usize::MAX; 1 << 15];
    let mut prev = vec![usize::MAX; data.len()];
    let insert = |at: usize, head: &mut Vec<usize>, prev: &mut Vec<usize>| {
        if at + 3 <= data.len() {
            let h = hash(at) as usize;
            prev[at] = head[h];
            head[h] = at;
        }
    };

    let mut at = 0;
    while at < data.len() {
        let flags_at = out.len();
        out.push(0);
        for bit in (0..8).rev() {
            if at >= data.len() {
                break;
            }
            let (mut best, mut disp) = (0, 0);
            if at + 3 <= data.len() {
                let limit = (data.len() - at).min(MAX_LEN);
                let mut candidate = head[hash(at) as usize];
                for _ in 0..CHAIN {
                    if candidate == usize::MAX || at - candidate > WINDOW {
                        break;
                    }
                    let len = (0..limit).take_while(|&i| data[candidate + i] == data[at + i]).count();
                    if len > best {
                        (best, disp) = (len, at - candidate);
                        if len == limit {
                            break;
                        }
                    }
                    candidate = prev[candidate];
                }
            }
            if best < 3 {
                out.push(data[at]);
                insert(at, &mut head, &mut prev);
                at += 1;
                continue;
            }
            out[flags_at] |= 1 << bit;
            let d = disp - 1;
            match best {
                3..=0x10 => out.extend_from_slice(&[((best - 1) << 4 | d >> 8) as u8, d as u8]),
                0x11..=0x110 => {
                    let n = best - 0x11;
                    out.extend_from_slice(&[(n >> 4) as u8, ((n & 0xF) << 4 | d >> 8) as u8, d as u8]);
                }
                _ => {
                    let n = best - 0x111;
                    out.extend_from_slice(&[(0x10 | n >> 12) as u8, (n >> 4) as u8, ((n & 0xF) << 4 | d >> 8) as u8, d as u8]);
                }
            }
            for i in at..at + best {
                insert(i, &mut head, &mut prev);
            }
            at += best;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compresses_what_it_decompresses() {
        let mut text = b"abcabcabcabc! Wii Party Wii Party ".repeat(40);
        text.extend((0..70_000u32).map(|i| (i * 7 % 251) as u8));
        text.extend(std::iter::repeat_n(0u8, 0x10200));
        for data in [&b""[..], b"a", b"abcabcabcabc!", &text] {
            let packed = compress(data);
            assert_eq!(decompress(&packed).unwrap(), data);
        }
        assert!(compress(&text).len() < text.len() / 2);
    }
}
