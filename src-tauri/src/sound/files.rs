//! Files inside a sound archive: RWAR (wave archive), RWSD (wave sounds), only what the merge needs
//! (wp-sound tools/wpsound/files.py). Both are written the way the game's are: rebuilding any RWAR
//! or RWSD of wp_sound.brsar from what is read here gives back the same bytes.

use super::rsar::{align, put_u32};

fn read(d: &[u8], o: usize) -> Result<u32, String> {
    d.get(o..o + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap())).ok_or_else(|| "fichier tronqué".to_string())
}

fn f32_at(d: &[u8], o: usize) -> Result<f32, String> {
    read(d, o).map(f32::from_bits)
}

fn pad(b: &mut Vec<u8>, a: usize) {
    b.resize(align(b.len(), a), 0);
}

// ===================================================================================== RWAR

/// RWAR -> the RWAV of each wave.
pub fn rwar_split(d: &[u8]) -> Result<Vec<Vec<u8>>, String> {
    if d.get(..4) != Some(b"RWAR") {
        return Err("pas un RWAR".into());
    }
    let tabl = read(d, 0x10)? as usize;
    let data = read(d, 0x18)? as usize;
    let n = read(d, tabl + 8)? as usize;
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let (off, size) = (read(d, tabl + 12 + 12 * i + 4)? as usize, read(d, tabl + 12 + 12 * i + 8)? as usize);
        out.push(d.get(data + off..data + off + size).ok_or("RWAR tronqué")?.to_vec());
    }
    Ok(out)
}

pub fn build_rwar(rwavs: &[Vec<u8>]) -> Vec<u8> {
    let n = rwavs.len();
    let tabl_size = align(12 + 12 * n, 0x20);
    let mut body = b"DATA".to_vec();
    body.resize(0x20, 0);
    let mut tabl = b"TABL".to_vec();
    tabl.extend_from_slice(&(tabl_size as u32).to_be_bytes());
    tabl.extend_from_slice(&(n as u32).to_be_bytes());
    for w in rwavs {
        for v in [0x0100_0000, body.len() as u32, w.len() as u32] {
            tabl.extend_from_slice(&v.to_be_bytes());
        }
        body.extend_from_slice(w);
        pad(&mut body, 0x20);
    }
    pad(&mut tabl, 0x20);
    let body_size = body.len() as u32;
    put_u32(&mut body, 4, body_size);
    let mut out = b"RWAR".to_vec();
    out.extend_from_slice(&[0xFE, 0xFF, 0x01, 0x00, 0, 0, 0, 0, 0x00, 0x20, 0x00, 0x02]);
    for v in [0x20, tabl_size as u32, 0x20 + tabl_size as u32, body.len() as u32] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&tabl);
    out.extend_from_slice(&body);
    let size = out.len() as u32;
    put_u32(&mut out, 8, size);
    out
}

// ===================================================================================== RWSD

/// A wave sound's note: the wave it plays and its envelope.
#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub wave: i32,
    /// attack, decay, sustain, release, hold, wave_location, note_off, alt_assign, key, volume, pan,
    /// surround_pan (NOTE_BYTES)
    pub bytes: [u8; 12],
    pub tune: f32,
}

pub const NOTE_BYTES: [&str; 12] = ["attack", "decay", "sustain", "release", "hold", "wave_location", "note_off", "alt_assign", "key", "volume", "pan", "surround_pan"];

/// A wave sound: its info (pitch, pan, sends), one track with one event playing one note. Every wave
/// sound of the game has this shape (343/343).
#[derive(Clone, Debug, PartialEq)]
pub struct Wsd {
    pub pitch: f32,
    pub pan: u8,
    pub surround_pan: u8,
    pub fx_send: [u8; 3],
    pub main_send: u8,
    /// (pos, length)
    pub event: (f32, f32),
    pub note: Note,
}

pub fn read_rwsd(d: &[u8]) -> Result<Vec<Wsd>, String> {
    if d.get(..4) != Some(b"RWSD") {
        return Err("pas un RWSD".into());
    }
    let base = read(d, 0x10)? as usize + 8;
    let r = |o: usize| read(d, o + 4).map(|v| base + v as usize);
    let mut out = Vec::new();
    for i in 0..read(d, base)? as usize {
        let e = r(base + 4 + 8 * i)?;
        let (info, tracks, notes) = (r(e)?, r(e + 8)?, r(e + 16)?);
        if read(d, tracks)? != 1 || read(d, notes)? != 1 {
            return Err("son d'onde à plusieurs pistes ou notes : non pris en charge".into());
        }
        let evs = r(r(tracks + 4)?)?;
        if read(d, evs)? != 1 {
            return Err("piste à plusieurs événements : non prise en charge".into());
        }
        let ev = r(evs + 4)?;
        let note = r(notes + 4)?;
        let b = d.get(info + 4..info + 10).ok_or("RWSD tronqué")?;
        let nb = d.get(note + 4..note + 16).ok_or("RWSD tronqué")?;
        out.push(Wsd {
            pitch: f32_at(d, info)?,
            pan: b[0],
            surround_pan: b[1],
            fx_send: [b[2], b[3], b[4]],
            main_send: b[5],
            event: (f32_at(d, ev)?, f32_at(d, ev + 4)?),
            note: Note { wave: read(d, note)? as i32, bytes: nb.try_into().unwrap(), tune: f32_at(d, note + 16)? },
        });
        if read(d, ev + 8)? != 0 {
            return Err("événement sur une autre note que la 0".into());
        }
    }
    Ok(out)
}

pub fn build_rwsd(sounds: &[Wsd]) -> Vec<u8> {
    let mut body = (sounds.len() as u32).to_be_bytes().to_vec();
    body.resize(4 + 8 * sounds.len(), 0);
    fn put_ref(body: &mut [u8], at: usize, target: usize) {
        body[at..at + 4].copy_from_slice(&[1, 0, 0, 0]);
        put_u32(body, at + 4, target as u32);
    }
    let count = |body: &mut Vec<u8>| {
        body.extend_from_slice(&1u32.to_be_bytes());
        body.extend_from_slice(&[0; 8]);
    };
    for (i, s) in sounds.iter().enumerate() {
        let e = body.len();
        put_ref(&mut body, 4 + 8 * i, e);
        body.resize(body.len() + 0x18, 0);
        let info = body.len();
        body.extend_from_slice(&s.pitch.to_bits().to_be_bytes());
        body.extend_from_slice(&[s.pan, s.surround_pan, s.fx_send[0], s.fx_send[1], s.fx_send[2], s.main_send]);
        body.resize(body.len() + 0x20 - 10, 0);
        let tracks = body.len();
        count(&mut body);
        let track = body.len();
        body.resize(body.len() + 8, 0);
        let evs = body.len();
        count(&mut body);
        let ev = body.len();
        body.extend_from_slice(&s.event.0.to_bits().to_be_bytes());
        body.extend_from_slice(&s.event.1.to_bits().to_be_bytes());
        body.extend_from_slice(&[0; 8]);
        let notes = body.len();
        count(&mut body);
        let note = body.len();
        body.extend_from_slice(&(s.note.wave as u32).to_be_bytes());
        body.extend_from_slice(&s.note.bytes);
        body.extend_from_slice(&s.note.tune.to_bits().to_be_bytes());
        body.resize(body.len() + 0x30 - 20, 0);
        for (at, target) in [(e, info), (e + 8, tracks), (e + 16, notes), (tracks + 4, track), (track, evs), (evs + 4, ev), (notes + 4, note)] {
            put_ref(&mut body, at, target);
        }
    }
    let mut data = b"DATA\0\0\0\0".to_vec();
    data.extend_from_slice(&body);
    let size = align(0x20 + data.len(), 0x20) - 0x20;
    data.resize(size, 0);
    put_u32(&mut data, 4, size as u32);
    let mut out = b"RWSD".to_vec();
    out.extend_from_slice(&[0xFE, 0xFF, 0x01, 0x03]);
    for v in [0x20 + size as u32, 0x0020_0001, 0x20, size as u32, 0, 0] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&data);
    out
}
