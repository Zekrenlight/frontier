//! A small PNG reader for block textures: zlib inflate (stored, fixed and dynamic Huffman
//! blocks) and the five scanline filters, for 8-bit greyscale, grey with alpha, RGB, RGBA and
//! palette images (with `tRNS`). Enough for the game's and resource packs' textures; anything
//! else (16-bit, interlaced) is refused and the block falls back to a plain colour.

/// A decoded image: `width * height` RGBA pixels, row by row from the top left.
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
    nbits: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            pos: 0,
            bit: 0,
            nbits: 0,
        }
    }
    fn need(&mut self, n: u32) -> Option<()> {
        while self.nbits < n {
            let byte = *self.data.get(self.pos)?;
            self.pos += 1;
            self.bit |= u32::from(byte) << self.nbits;
            self.nbits += 8;
        }
        Some(())
    }
    fn take(&mut self, n: u32) -> Option<u32> {
        if n == 0 {
            return Some(0);
        }
        self.need(n)?;
        let v = self.bit & ((1u32 << n) - 1);
        self.bit >>= n;
        self.nbits -= n;
        Some(v)
    }
    fn align(&mut self) {
        let drop = self.nbits % 8;
        self.bit >>= drop;
        self.nbits -= drop;
    }
}

/// A canonical Huffman code as counts per length and symbols in code order.
struct Code {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl Code {
    fn new(lengths: &[u8]) -> Option<Self> {
        let mut counts = [0u16; 16];
        for &l in lengths {
            counts[usize::from(l)] += 1;
        }
        counts[0] = 0;
        let mut offsets = [0u16; 16];
        for i in 1..16 {
            offsets[i] = offsets[i - 1] + counts[i - 1];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (s, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[usize::from(offsets[usize::from(l)])] = s as u16;
                offsets[usize::from(l)] += 1;
            }
        }
        Some(Self { counts, symbols })
    }
    fn decode(&self, bits: &mut Bits<'_>) -> Option<u16> {
        // Walk the code one bit at a time: `first` is the first code of this length and
        // `index` the first symbol slot of it.
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..16 {
            code |= bits.take(1)? as i32;
            let count = i32::from(self.counts[len]);
            if code - first < count {
                return self.symbols.get((index + code - first) as usize).copied();
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// Inflates a zlib stream (the two-byte header is skipped, the checksum not checked), giving up
/// past `limit` bytes of output.
pub fn inflate_zlib(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    if data.len() < 2 || (u16::from(data[0]) << 8 | u16::from(data[1])) % 31 != 0 {
        return None;
    }
    inflate(&data[2..], limit)
}

/// Inflates a raw deflate stream.
pub fn inflate(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    let mut bits = Bits::new(data);
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = bits.take(1)? == 1;
        match bits.take(2)? {
            0 => {
                bits.align();
                let len = bits.take(16)? as usize;
                let _nlen = bits.take(16)?;
                for _ in 0..len {
                    out.push(bits.take(8)? as u8);
                }
            }
            kind @ (1 | 2) => {
                let (lit, dist) = if kind == 1 {
                    let mut l = [0u8; 288];
                    for (i, v) in l.iter_mut().enumerate() {
                        *v = match i {
                            0..=143 => 8,
                            144..=255 => 9,
                            256..=279 => 7,
                            _ => 8,
                        };
                    }
                    (Code::new(&l)?, Code::new(&[5u8; 30])?)
                } else {
                    dynamic_codes(&mut bits)?
                };
                loop {
                    let sym = lit.decode(&mut bits)?;
                    if sym < 256 {
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let i = usize::from(sym - 257);
                        let len = usize::from(*LEN_BASE.get(i)?)
                            + bits.take(u32::from(LEN_EXTRA[i]))? as usize;
                        let d = usize::from(dist.decode(&mut bits)?);
                        let back = usize::from(*DIST_BASE.get(d)?)
                            + bits.take(u32::from(DIST_EXTRA[d]))? as usize;
                        if back > out.len() {
                            return None;
                        }
                        let start = out.len() - back;
                        for k in 0..len {
                            let b = out[start + k];
                            out.push(b);
                        }
                    }
                    if out.len() > limit {
                        return None;
                    }
                }
            }
            _ => return None,
        }
        if out.len() > limit {
            return None;
        }
        if last {
            return Some(out);
        }
    }
}

fn dynamic_codes(bits: &mut Bits<'_>) -> Option<(Code, Code)> {
    const ORDER: [usize; 19] = [
        16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
    ];
    let hlit = bits.take(5)? as usize + 257;
    let hdist = bits.take(5)? as usize + 1;
    let hclen = bits.take(4)? as usize + 4;
    let mut cl = [0u8; 19];
    for &o in ORDER.iter().take(hclen) {
        cl[o] = bits.take(3)? as u8;
    }
    let clc = Code::new(&cl)?;
    let mut lengths = Vec::with_capacity(hlit + hdist);
    while lengths.len() < hlit + hdist {
        let sym = clc.decode(bits)?;
        match sym {
            0..=15 => lengths.push(sym as u8),
            16 => {
                let prev = *lengths.last()?;
                for _ in 0..3 + bits.take(2)? {
                    lengths.push(prev);
                }
            }
            17 => {
                for _ in 0..3 + bits.take(3)? {
                    lengths.push(0);
                }
            }
            _ => {
                for _ in 0..11 + bits.take(7)? {
                    lengths.push(0);
                }
            }
        }
    }
    if lengths.len() > hlit + hdist {
        return None;
    }
    Some((Code::new(&lengths[..hlit])?, Code::new(&lengths[hlit..])?))
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

/// Decodes a PNG file. None for anything this reader does not handle.
pub fn decode(file: &[u8]) -> Option<Image> {
    const SIG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    if file.len() < 8 || file[..8] != SIG {
        return None;
    }
    let mut at = 8;
    let (mut width, mut height, mut depth, mut color, mut interlace) = (0u32, 0u32, 0u8, 0u8, 0u8);
    let mut palette: Vec<[u8; 4]> = Vec::new();
    let mut trns: Vec<u8> = Vec::new();
    let mut idat: Vec<u8> = Vec::new();
    while at + 8 <= file.len() {
        let len = be32(&file[at..]) as usize;
        let kind = &file[at + 4..at + 8];
        let body = file.get(at + 8..at + 8 + len)?;
        match kind {
            b"IHDR" if len >= 13 => {
                width = be32(body);
                height = be32(&body[4..]);
                depth = body[8];
                color = body[9];
                interlace = body[12];
            }
            b"PLTE" => {
                palette = body
                    .chunks_exact(3)
                    .map(|c| [c[0], c[1], c[2], 255])
                    .collect();
            }
            b"tRNS" => trns = body.to_vec(),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        at += 12 + len;
    }
    let channels: usize = match color {
        0 => 1,
        2 => 3,
        3 => 1,
        4 => 2,
        6 => 4,
        _ => return None,
    };
    // 1, 2 and 4 bits a sample for greyscale and palette images (the game's own small
    // textures are mostly 4-bit palettes), 8 for all, 16 for the colour kinds.
    let depth_ok = match color {
        0 => matches!(depth, 1 | 2 | 4 | 8 | 16),
        3 => matches!(depth, 1 | 2 | 4 | 8),
        _ => matches!(depth, 8 | 16),
    };
    if width == 0 || height == 0 || width > 4096 || height > 16384 || !depth_ok || interlace != 0 {
        return None;
    }
    for (i, a) in trns.iter().enumerate() {
        if let Some(p) = palette.get_mut(i) {
            p[3] = *a;
        }
    }
    let (w, h) = (width as usize, height as usize);
    let bits = channels * usize::from(depth);
    let stride = (w * bits).div_ceil(8);
    // The filters work on whole bytes: a pixel's bytes, at least one.
    let bpp = bits.div_ceil(8).max(1);
    let raw = inflate_zlib(&idat, (stride + 1) * h)?;
    if raw.len() < (stride + 1) * h {
        return None;
    }
    let mut cur = vec![0u8; stride];
    let mut prev = vec![0u8; stride];
    let mut rgba = Vec::with_capacity(w * h * 4);
    let mut samples = vec![0u8; channels];
    for y in 0..h {
        let row = &raw[y * (stride + 1)..(y + 1) * (stride + 1)];
        let filter = row[0];
        for i in 0..stride {
            let x = row[1 + i];
            let a = if i >= bpp { cur[i - bpp] } else { 0 };
            let b = prev[i];
            let c = if i >= bpp { prev[i - bpp] } else { 0 };
            cur[i] = match filter {
                0 => x,
                1 => x.wrapping_add(a),
                2 => x.wrapping_add(b),
                3 => x.wrapping_add(((u16::from(a) + u16::from(b)) / 2) as u8),
                4 => x.wrapping_add(paeth(a, b, c)),
                _ => return None,
            };
        }
        for xp in 0..w {
            for (ch, sample) in samples.iter_mut().enumerate() {
                *sample = sample_at(
                    &cur,
                    (xp * channels + ch) * usize::from(depth),
                    depth,
                    color == 3,
                );
            }
            let px = &samples;
            let p = match color {
                0 => [px[0], px[0], px[0], 255],
                2 => [px[0], px[1], px[2], 255],
                3 => *palette.get(usize::from(px[0]))?,
                4 => [px[0], px[0], px[0], px[1]],
                _ => [px[0], px[1], px[2], px[3]],
            };
            rgba.extend_from_slice(&p);
        }
        std::mem::swap(&mut cur, &mut prev);
    }
    Some(Image {
        width,
        height,
        rgba,
    })
}

/// The sample `bit` bits into a row: a palette index as it is, a grey or colour level scaled
/// to 8 bits (16-bit samples keep their high byte).
fn sample_at(row: &[u8], bit: usize, depth: u8, index: bool) -> u8 {
    match depth {
        8 => row[bit / 8],
        16 => row[bit / 8],
        d => {
            let byte = row[bit / 8];
            let shift = 8 - usize::from(d) - (bit % 8);
            let v = (byte >> shift) & ((1u8 << d) - 1);
            if index {
                v
            } else {
                // 1 -> 255, 2 bits -> x 85, 4 bits -> x 17.
                v * (255 / ((1u8 << d) - 1))
            }
        }
    }
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let p = i16::from(a) + i16::from(b) - i16::from(c);
    let (pa, pb, pc) = (
        (p - i16::from(a)).abs(),
        (p - i16::from(b)).abs(),
        (p - i16::from(c)).abs(),
    );
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

/// Writes RGBA pixels as a PNG with stored (uncompressed) deflate blocks: for the tools that
/// preview the frames, not used in game.
pub fn encode_stored(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    fn crc(data: &[u8]) -> u32 {
        let mut c = 0xFFFF_FFFFu32;
        for &b in data {
            c ^= u32::from(b);
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
        }
        !c
    }
    fn chunk(out: &mut Vec<u8>, kind: &[u8], body: &[u8]) {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        let mut k = kind.to_vec();
        k.extend_from_slice(body);
        out.extend_from_slice(&k);
        out.extend_from_slice(&crc(&k).to_be_bytes());
    }
    let mut raw = Vec::new();
    for row in rgba.chunks_exact(width as usize * 4) {
        raw.push(0);
        raw.extend_from_slice(row);
    }
    let mut z = vec![0x78, 0x01];
    let mut blocks = raw.chunks(65535).peekable();
    while let Some(b) = blocks.next() {
        z.push(u8::from(blocks.peek().is_none()));
        z.extend_from_slice(&(b.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(b.len() as u16)).to_le_bytes());
        z.extend_from_slice(b);
    }
    let (mut s1, mut s2) = (1u32, 0u32);
    for &b in &raw {
        s1 = (s1 + u32::from(b)) % 65521;
        s2 = (s2 + s1) % 65521;
    }
    z.extend_from_slice(&((s2 << 16) | s1).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&width.to_be_bytes());
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
