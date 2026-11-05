//! The parts of RFLiCharInfo the head is made of, read from the 74-byte RFLiCharData
//! (RFL_Database.c: convertRaw2InfoCore_). Its bitfields are big-endian u16s, first
//! field in the high bits.

#[derive(Clone, Copy, Default)]
pub struct Info {
    pub favorite_color: u8,
    pub faceline_type: u8,
    pub faceline_color: u8,
    pub faceline_texture: u8,
    pub hair_type: u8,
    pub hair_color: u8,
    pub hair_flip: bool,
    pub eye: Part,
    pub eyebrow: Part,
    pub nose_type: u8,
    pub nose_scale: u8,
    pub nose_y: u8,
    pub mouth_type: u8,
    pub mouth_color: u8,
    pub mouth_scale: u8,
    pub mouth_y: u8,
    pub mustache: u8,
    pub beard_type: u8,
    pub beard_color: u8,
    pub beard_scale: u8,
    pub beard_y: u8,
    pub glass_type: u8,
    pub glass_color: u8,
    pub glass_scale: u8,
    pub glass_y: u8,
    pub mole_type: u8,
    pub mole_scale: u8,
    pub mole_x: u8,
    pub mole_y: u8,
}

/// An eye or an eyebrow.
#[derive(Clone, Copy, Default)]
pub struct Part {
    pub kind: u8,
    pub color: u8,
    pub scale: u8,
    pub rotate: u8,
    pub x: u8,
    pub y: u8,
}

impl Info {
    pub fn parse(data: &[u8; 74]) -> Result<Info, String> {
        // `len` bits of the u16 at `at`, `skip` bits below its top.
        let f = |at: usize, skip: u32, len: u32| ((u16::from_be_bytes([data[at], data[at + 1]]) << skip) >> (16 - len)) as u8;
        let info = Info {
            favorite_color: f(0x00, 11, 4),
            faceline_type: f(0x20, 0, 3),
            faceline_color: f(0x20, 3, 3),
            faceline_texture: f(0x20, 6, 4),
            hair_type: f(0x22, 0, 7),
            hair_color: f(0x22, 7, 3),
            hair_flip: f(0x22, 10, 1) != 0,
            eyebrow: Part {
                kind: f(0x24, 0, 5),
                rotate: f(0x24, 5, 5),
                color: f(0x26, 0, 3),
                scale: f(0x26, 3, 4),
                y: f(0x26, 7, 5),
                x: f(0x26, 12, 4),
            },
            eye: Part {
                kind: f(0x28, 0, 6),
                rotate: f(0x28, 6, 5),
                y: f(0x28, 11, 5),
                color: f(0x2A, 0, 3),
                scale: f(0x2A, 3, 4),
                x: f(0x2A, 7, 4),
            },
            nose_type: f(0x2C, 0, 4),
            nose_scale: f(0x2C, 4, 4),
            nose_y: f(0x2C, 8, 5),
            mouth_type: f(0x2E, 0, 5),
            mouth_color: f(0x2E, 5, 2),
            mouth_scale: f(0x2E, 7, 4),
            mouth_y: f(0x2E, 11, 5),
            glass_type: f(0x30, 0, 4),
            glass_color: f(0x30, 4, 3),
            glass_scale: f(0x30, 7, 4),
            glass_y: f(0x30, 11, 5),
            mustache: f(0x32, 0, 2),
            beard_type: f(0x32, 2, 2),
            beard_color: f(0x32, 4, 3),
            beard_scale: f(0x32, 7, 4),
            beard_y: f(0x32, 11, 5),
            mole_type: f(0x34, 0, 1),
            mole_scale: f(0x34, 1, 4),
            mole_y: f(0x34, 5, 5),
            mole_x: f(0x34, 10, 5),
        };
        if info.valid() {
            Ok(info)
        } else {
            Err("invalid Mii data".into())
        }
    }

    /// The appearance half of RFLiCheckValidInfo (RFL_DataUtility.c): RFL draws nothing else.
    fn valid(&self) -> bool {
        let (e, b) = (&self.eye, &self.eyebrow);
        self.favorite_color < 12
            && self.faceline_type <= 7
            && self.faceline_color <= 5
            && self.faceline_texture <= 11
            && self.hair_type <= 71
            && e.kind <= 47
            && e.color <= 5
            && e.scale <= 7
            && e.rotate <= 7
            && e.x <= 12
            && e.y <= 18
            && b.kind <= 23
            && b.scale <= 8
            && b.rotate <= 11
            && b.x <= 12
            && (3..=18).contains(&b.y)
            && self.nose_type <= 11
            && self.nose_scale <= 8
            && self.nose_y <= 18
            && self.mouth_type <= 23
            && self.mouth_color <= 2
            && self.mouth_scale <= 8
            && self.mouth_y <= 18
            && self.beard_scale <= 8
            && self.beard_y <= 16
            && self.glass_type <= 8
            && self.glass_color <= 5
            && self.glass_scale <= 7
            && self.glass_y <= 20
            && self.mole_scale <= 8
            && self.mole_x <= 16
            && self.mole_y <= 30
    }
}
