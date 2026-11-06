//! The game's message files (locale/<language>/mess.bin, "XMSG"), so that several mods can change their
//! texts: each mod gives keys and texts, the launcher applies them all to the disc's file (wp-winter
//! tools/xmsg.py).
//!
//! "XMSG", u32 (a date), u32 entry count, then 16-byte entries (key, text, sound key, style: absolute
//! offsets, after the table), sorted by key (the game looks them up by bisection, strcmp). Keys are
//! latin-1, texts UTF-16 big-endian ending with 0. A new text goes at the end of the file and its entry
//! points at it: no other offset moves. A new key is put at its place in the table, every offset moving
//! by 16; its sound key and style are those of the first key of the same shape (MG113_rule: MG100_rule).

pub struct Xmsg {
    data: Vec<u8>,
}

fn u32_at(d: &[u8], o: usize) -> Option<u32> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()))
}

/// The digits of a key, whatever they are: "MG113_rule" and "MG100_rule" have the same shape.
fn shape(key: &str) -> String {
    let mut out = String::new();
    for c in key.chars() {
        if c.is_ascii_digit() {
            if !out.ends_with('#') {
                out.push('#');
            }
        } else {
            out.push(c);
        }
    }
    out
}

impl Xmsg {
    pub fn parse(data: Vec<u8>) -> Result<Xmsg, String> {
        if data.get(..4) != Some(b"XMSG") {
            return Err("pas un fichier XMSG".into());
        }
        let m = Xmsg { data };
        let count = m.count();
        if 12 + count * 16 > m.data.len() || (0..count).any(|k| m.key(k).is_none()) {
            return Err("fichier XMSG abîmé".into());
        }
        Ok(m)
    }

    fn count(&self) -> usize {
        u32_at(&self.data, 8).unwrap_or(0) as usize
    }

    fn entry(&self, k: usize) -> [u32; 4] {
        let o = 12 + k * 16;
        [0, 4, 8, 12].map(|i| u32_at(&self.data, o + i).unwrap_or(0))
    }

    fn key(&self, k: usize) -> Option<&[u8]> {
        let o = self.entry(k)[0] as usize;
        let rest = self.data.get(o..)?;
        Some(&rest[..rest.iter().position(|&b| b == 0)?])
    }

    /// The entry of `key` (latin-1), or where it would go.
    fn find(&self, key: &[u8]) -> Result<usize, usize> {
        let (mut lo, mut hi) = (0, self.count());
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.key(mid).unwrap_or_default().cmp(key) {
                std::cmp::Ordering::Equal => return Ok(mid),
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
            }
        }
        Err(lo)
    }

    #[allow(dead_code)] // (tests)
    pub fn get(&self, key: &str) -> Option<String> {
        let k = self.find(&latin1(key)?).ok()?;
        let o = self.entry(k)[1] as usize;
        let units: Vec<u16> = self.data.get(o..)?.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).take_while(|&u| u != 0).collect();
        Some(String::from_utf16_lossy(&units))
    }

    /// Changes the text of `key`, or adds the key (with the sound and style of a key of the same shape).
    pub fn set(&mut self, key: &str, text: &str) -> Result<(), String> {
        let name = latin1(key).ok_or_else(|| format!("clé de message « {key} » invalide"))?;
        let k = match self.find(&name) {
            Ok(k) => k,
            Err(at) => self.add(&name, at).ok_or_else(|| format!("clé de message « {key} » inconnue du jeu (et aucune de même forme)"))?,
        };
        if self.data.len() % 2 != 0 {
            self.data.push(0);
        }
        let off = self.data.len() as u32;
        for u in text.encode_utf16().chain([0]) {
            self.data.extend_from_slice(&u.to_be_bytes());
        }
        let o = 12 + k * 16 + 4;
        self.data[o..o + 4].copy_from_slice(&off.to_be_bytes());
        Ok(())
    }

    /// A new entry at `at`; its text is set afterwards.
    fn add(&mut self, key: &[u8], at: usize) -> Option<usize> {
        let wanted = shape(&String::from_utf8_lossy(key));
        let count = self.count();
        let mut like: Option<usize> = None;
        for k in 0..count {
            let name = String::from_utf8_lossy(self.key(k)?).into_owned();
            // (the table is sorted: the first of that shape)
            if shape(&name) == wanted {
                like = Some(k);
                break;
            }
        }
        let [_, _, sound, style] = self.entry(like?);
        let table_end = 12 + count * 16;
        let mut entries: Vec<[u32; 4]> = (0..count).map(|k| self.entry(k).map(|o| o + 16)).collect();
        let mut rest = self.data[table_end..].to_vec();
        let key_off = (table_end + 16 + rest.len()) as u32;
        entries.insert(at, [key_off, 0, sound + 16, style + 16]);
        rest.extend_from_slice(key);
        rest.push(0);
        let mut data = self.data[..8].to_vec();
        data.extend_from_slice(&(count as u32 + 1).to_be_bytes());
        for e in &entries {
            for o in e {
                data.extend_from_slice(&o.to_be_bytes());
            }
        }
        data.extend_from_slice(&rest);
        self.data = data;
        Some(at)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
}

fn latin1(text: &str) -> Option<Vec<u8>> {
    text.chars().map(|c| u8::try_from(c as u32).ok().filter(|&b| b != 0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(entries: &[(&str, &str)]) -> Vec<u8> {
        // keys, then texts, then one shared sound key and style
        let n = entries.len();
        let mut tail = Vec::new();
        let base = 12 + n * 16;
        let mut table = Vec::new();
        let sound = base;
        tail.extend_from_slice(b"snd\0");
        let style = base + tail.len();
        tail.extend_from_slice(&[7u8; 16]);
        for (k, t) in entries {
            let ko = base + tail.len();
            tail.extend_from_slice(k.as_bytes());
            tail.push(0);
            if tail.len() % 2 != 0 {
                tail.push(0);
            }
            let to = base + tail.len();
            for u in t.encode_utf16().chain([0]) {
                tail.extend_from_slice(&u.to_be_bytes());
            }
            table.push([ko, to, sound, style]);
        }
        let mut d = b"XMSG".to_vec();
        d.extend_from_slice(&0u32.to_be_bytes());
        d.extend_from_slice(&(n as u32).to_be_bytes());
        for e in table {
            for o in e {
                d.extend_from_slice(&(o as u32).to_be_bytes());
            }
        }
        d.extend_from_slice(&tail);
        d
    }

    #[test]
    fn changes_and_adds_texts() {
        let mut m = Xmsg::parse(file(&[("MG100_rule", "règle"), ("m001_004", "Bonne chance !")])).unwrap();
        assert_eq!(m.get("m001_004").as_deref(), Some("Bonne chance !"));
        m.set("m001_004", "Bonne chance, [c:#e41464ff]vraiment[c:#3c3c3cff] !\nVite").unwrap();
        assert_eq!(m.get("m001_004").as_deref(), Some("Bonne chance, [c:#e41464ff]vraiment[c:#3c3c3cff] !\nVite"));
        m.set("MG113_rule", "nouvelle").unwrap();
        let m = Xmsg::parse(m.bytes().to_vec()).unwrap();
        assert_eq!(m.count(), 3);
        assert_eq!(m.get("MG113_rule").as_deref(), Some("nouvelle"));
        assert_eq!(m.get("MG100_rule").as_deref(), Some("règle"));
        assert_eq!(m.key(1), Some(&b"MG113_rule"[..]));
        // the new key has the sound and style of its model
        assert_eq!(m.entry(1)[2..], m.entry(0)[2..]);
        let mut m = m;
        assert!(m.set("zz", "x").is_err());
    }
}
