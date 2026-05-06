//! QR code generator optimized for URLs.
//!
//! Byte mode, EC-L, versions 1-6, fixed mask pattern.
//! Supports up to 134 character URLs.
//!
//! Embedded from https://github.com/Anaconda-Sandbox/url2qr (BSD-3-Clause).

use crate::errors::QrError;

// ---------------------------------------------------------------------------
// GF(2^8) arithmetic for Reed-Solomon
// ---------------------------------------------------------------------------

/// Precomputed GF(2^8) exponent table (512 entries for wraparound).
fn gf_exp_table() -> [u8; 512] {
    let mut table = [0u8; 512];
    let mut x: u16 = 1;
    for entry in table.iter_mut().take(255) {
        *entry = x as u8;
        x <<= 1;
        if x & 0x100 != 0 {
            x ^= 0x11D;
        }
    }
    // Copy for wraparound: table[255..512] = table[0..257]
    // We only need 255..510 range for max log sum (254+254=508)
    for i in 255..512 {
        table[i] = table[i - 255];
    }
    table
}

/// Precomputed GF(2^8) logarithm table.
fn gf_log_table(exp: &[u8; 512]) -> [u8; 256] {
    let mut table = [0u8; 256];
    for i in 0..255 {
        table[exp[i] as usize] = i as u8;
    }
    table
}

struct GfTables {
    exp: [u8; 512],
    log: [u8; 256],
}

impl GfTables {
    fn new() -> Self {
        let exp = gf_exp_table();
        let log = gf_log_table(&exp);
        Self { exp, log }
    }

    fn mul(&self, a: u8, b: u8) -> u8 {
        if a == 0 || b == 0 {
            0
        } else {
            self.exp[self.log[a as usize] as usize + self.log[b as usize] as usize]
        }
    }

    fn poly_mul(&self, p: &[u8], q: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; p.len() + q.len() - 1];
        for (i, &a) in p.iter().enumerate() {
            for (j, &b) in q.iter().enumerate() {
                r[i + j] ^= self.mul(a, b);
            }
        }
        r
    }

    fn poly_div(&self, dividend: &[u8], divisor: &[u8]) -> Vec<u8> {
        let mut r = dividend.to_vec();
        let steps = dividend.len() - divisor.len() + 1;
        for i in 0..steps {
            if r[i] != 0 {
                for j in 1..divisor.len() {
                    if divisor[j] != 0 {
                        r[i + j] ^= self.mul(divisor[j], r[i]);
                    }
                }
            }
        }
        r[r.len() - (divisor.len() - 1)..].to_vec()
    }

    fn rs_encode(&self, data: &[u8], nsym: usize) -> Vec<u8> {
        let mut g = vec![1u8];
        for i in 0..nsym {
            g = self.poly_mul(&g, &[1, self.exp[i]]);
        }
        let mut dividend = data.to_vec();
        dividend.extend(vec![0u8; nsym]);
        self.poly_div(&dividend, &g)
    }
}

// ---------------------------------------------------------------------------
// EC-L parameters
// ---------------------------------------------------------------------------

/// (total_cw, ec_per_block, num_blocks, data_cw_per_block)
const EC_PARAMS: [(usize, usize, usize, usize); 6] = [
    (26, 7, 1, 19),    // v1
    (44, 10, 1, 34),   // v2
    (70, 15, 1, 55),   // v3
    (100, 20, 1, 80),  // v4
    (134, 26, 1, 108), // v5
    (172, 18, 2, 68),  // v6
];

const DATA_CAPACITY: [usize; 6] = [17, 32, 53, 78, 106, 134];

const ALIGN_POSITIONS: [[u8; 2]; 6] = [
    [0, 0],  // v1: no alignment
    [6, 18], // v2
    [6, 22], // v3
    [6, 26], // v4
    [6, 30], // v5
    [6, 34], // v6
];

// ---------------------------------------------------------------------------
// Version selection
// ---------------------------------------------------------------------------

/// Select the smallest QR version (1-6) that fits the given URL.
pub fn select_version(data: &str) -> Result<u8, QrError> {
    // Verify all bytes are Latin-1 (all &str bytes are valid UTF-8,
    // but we need to ensure they fit in a single byte each).
    if data.chars().any(|c| c as u32 > 255) {
        return Err(QrError::InvalidByte);
    }
    let byte_len = data.len();
    for (v, &capacity) in DATA_CAPACITY.iter().enumerate() {
        if capacity >= byte_len {
            return Ok((v + 1) as u8);
        }
    }
    Err(QrError::TooLong(byte_len))
}

// ---------------------------------------------------------------------------
// Data encoding
// ---------------------------------------------------------------------------

/// Encode URL data into codewords with Reed-Solomon error correction.
pub fn encode_data(data: &str, version: u8) -> Vec<u8> {
    let gf = GfTables::new();
    let vi = (version as usize) - 1;
    let (_total, ec_per, nblocks, data_per) = EC_PARAMS[vi];
    let data_cw = nblocks * data_per;

    // Byte mode: 0100 + 8-bit length + data bytes
    let data_bytes = data.as_bytes();
    let mut bits = Vec::with_capacity(data_cw * 8 + 16);

    // Mode indicator: 0100
    bits.extend_from_slice(&[0, 1, 0, 0]);

    // 8-bit character count
    let len = data_bytes.len() as u8;
    for shift in (0..8).rev() {
        bits.push((len >> shift) & 1);
    }

    // Data bytes
    for &b in data_bytes {
        for shift in (0..8).rev() {
            bits.push((b >> shift) & 1);
        }
    }

    // Terminator: up to 4 zero bits
    let terminator_len = (data_cw * 8 - bits.len()).min(4);
    bits.extend(std::iter::repeat(0).take(terminator_len));

    // Pad to byte boundary
    if bits.len() % 8 != 0 {
        let pad = 8 - bits.len() % 8;
        bits.extend(std::iter::repeat(0).take(pad));
    }

    // Convert bits to codewords
    let mut cw: Vec<u8> = bits
        .chunks(8)
        .map(|chunk| chunk.iter().fold(0u8, |acc, &bit| (acc << 1) | bit))
        .collect();

    // Pad codewords with alternating 0xEC, 0x11
    let mut pad_idx = 0;
    while cw.len() < data_cw {
        cw.push(if pad_idx % 2 == 0 { 236 } else { 17 });
        pad_idx += 1;
    }

    // Split into blocks and add EC
    let mut blocks: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(nblocks);
    for i in 0..nblocks {
        let block_data = cw[i * data_per..(i + 1) * data_per].to_vec();
        let ec = gf.rs_encode(&block_data, ec_per);
        blocks.push((block_data, ec));
    }

    // Interleave data codewords then EC codewords
    let mut result = Vec::with_capacity(data_cw + nblocks * ec_per);
    for i in 0..data_per {
        for (data_block, _) in &blocks {
            if i < data_block.len() {
                result.push(data_block[i]);
            }
        }
    }
    for i in 0..ec_per {
        for (_, ec_block) in &blocks {
            result.push(ec_block[i]);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Matrix construction
// ---------------------------------------------------------------------------

/// QR code matrix with fixed-cell tracking.
struct Matrix {
    size: usize,
    data: Vec<Vec<u8>>,
    fixed: Vec<Vec<bool>>,
}

impl Matrix {
    fn new(version: u8) -> Self {
        let size = 17 + (version as usize) * 4;
        Self {
            size,
            data: vec![vec![0u8; size]; size],
            fixed: vec![vec![false; size]; size],
        }
    }

    fn set_fixed(&mut self, r: isize, c: isize, val: u8) {
        if r >= 0 && (r as usize) < self.size && c >= 0 && (c as usize) < self.size {
            let (ru, cu) = (r as usize, c as usize);
            self.data[ru][cu] = val;
            self.fixed[ru][cu] = true;
        }
    }

    fn place_finder(&mut self, pr: usize, pc: usize) {
        for r in 0..7 {
            for c in 0..7 {
                let val = if r == 0
                    || r == 6
                    || c == 0
                    || c == 6
                    || ((2..=4).contains(&r) && (2..=4).contains(&c))
                {
                    1
                } else {
                    0
                };
                self.set_fixed((pr + r) as isize, (pc + c) as isize, val);
            }
        }
    }

    fn place_separators(&mut self) {
        let s = self.size;
        for i in 0..8 {
            let ii = i as isize;
            self.set_fixed(7, ii, 0);
            self.set_fixed(ii, 7, 0);
            self.set_fixed(7, (s - 8 + i) as isize, 0);
            self.set_fixed(ii, (s - 8) as isize, 0);
            self.set_fixed((s - 8) as isize, ii, 0);
            self.set_fixed((s - 8 + i) as isize, 7, 0);
        }
    }

    fn place_timing(&mut self) {
        for i in 8..self.size - 8 {
            let val = if i % 2 == 0 { 1 } else { 0 };
            self.set_fixed(6, i as isize, val);
            self.set_fixed(i as isize, 6, val);
        }
    }

    fn place_alignment(&mut self, version: u8) {
        let vi = (version as usize) - 1;
        let positions = ALIGN_POSITIONS[vi];
        if positions[0] == 0 && positions[1] == 0 {
            return; // v1: no alignment pattern
        }
        // v2-6 have exactly one alignment pattern at (pos[1], pos[1])
        let center = positions[1] as isize;
        for dr in -2..=2isize {
            for dc in -2..=2isize {
                let val = if dr == -2 || dr == 2 || dc == -2 || dc == 2 || (dr == 0 && dc == 0) {
                    1
                } else {
                    0
                };
                self.set_fixed(center + dr, center + dc, val);
            }
        }
    }

    fn reserve_format_areas(&mut self) {
        let s = self.size;
        for i in 0..9 {
            self.fixed[8][i] = true;
            self.fixed[i][8] = true;
        }
        for i in 0..7 {
            self.fixed[s - 1 - i][8] = true;
        }
        for i in 0..8 {
            self.fixed[8][s - 8 + i] = true;
        }
    }

    fn place_dark_module(&mut self) {
        let r = self.size - 8;
        self.set_fixed(r as isize, 8, 1);
    }
}

/// Create matrix with finder patterns, timing, alignment, and reserved areas.
fn create_matrix(version: u8) -> Matrix {
    let mut m = Matrix::new(version);
    m.place_finder(0, 0);
    m.place_finder(0, m.size - 7);
    m.place_finder(m.size - 7, 0);
    m.place_separators();
    m.place_timing();
    m.place_dark_module();
    m.place_alignment(version);
    m.reserve_format_areas();
    m
}

/// Place data codewords in zigzag pattern.
fn place_data(m: &mut Matrix, codewords: &[u8]) {
    let size = m.size;
    let bits: Vec<u8> = codewords
        .iter()
        .flat_map(|&cw| (0..8).rev().map(move |shift| (cw >> shift) & 1))
        .collect();

    let mut bit_idx = 0usize;
    let mut col = size as isize - 1;
    let mut going_up = true;

    while col >= 0 && bit_idx < bits.len() {
        if col == 6 {
            col -= 1;
            continue;
        }

        for row_iter in 0..size {
            let row = if going_up {
                size - 1 - row_iter
            } else {
                row_iter
            };
            for &dc in &[0isize, -1isize] {
                let c = col + dc;
                if c >= 0 && (c as usize) < size {
                    let cu = c as usize;
                    if !m.fixed[row][cu] && bit_idx < bits.len() {
                        m.data[row][cu] = bits[bit_idx];
                        bit_idx += 1;
                    }
                }
            }
        }

        col -= 2;
        going_up = !going_up;
    }
}

/// Apply mask pattern 0: (row + col) % 2 == 0.
fn apply_mask(m: &mut Matrix) {
    for r in 0..m.size {
        for c in 0..m.size {
            if !m.fixed[r][c] && (r + c) % 2 == 0 {
                m.data[r][c] ^= 1;
            }
        }
    }
}

/// Place format information for EC-L + mask 0.
fn place_format_info(m: &mut Matrix) {
    let size = m.size;

    // Precomputed format info bits for EC-L + mask 0 (bit 14 to bit 0)
    const BITS: [u8; 15] = [1, 1, 1, 0, 1, 1, 1, 1, 1, 0, 0, 0, 1, 0, 0];

    // Copy 1: around top-left finder
    const COPY1: [(usize, usize); 15] = [
        (8, 0),
        (8, 1),
        (8, 2),
        (8, 3),
        (8, 4),
        (8, 5),
        (8, 7),
        (8, 8),
        (7, 8),
        (5, 8),
        (4, 8),
        (3, 8),
        (2, 8),
        (1, 8),
        (0, 8),
    ];
    for (i, &(r, c)) in COPY1.iter().enumerate() {
        m.data[r][c] = BITS[i];
    }

    // Copy 2: bottom-left column and top-right row
    for (i, &bit) in BITS.iter().enumerate().take(7) {
        m.data[size - 1 - i][8] = bit;
    }
    for (i, &bit) in BITS[7..].iter().enumerate() {
        m.data[8][size - 8 + i] = bit;
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Generate a QR code matrix for the given URL.
///
/// Returns a `Vec<Vec<u8>>` where 1 = dark module, 0 = light module.
pub fn generate_qr(url: &str) -> Result<Vec<Vec<u8>>, QrError> {
    let version = select_version(url)?;
    let codewords = encode_data(url, version);
    let mut m = create_matrix(version);
    place_data(&mut m, &codewords);
    apply_mask(&mut m);
    place_format_info(&mut m);
    Ok(m.data)
}

/// Generate a terminal-printable QR code using Unicode half-block characters.
pub fn qr_to_terminal(url: &str, quiet_zone: usize, invert: bool) -> Result<String, QrError> {
    let matrix = generate_qr(url)?;
    let size = matrix.len();

    let full_size = size + 2 * quiet_zone;
    let mut full = vec![vec![0u8; full_size]; full_size];
    for r in 0..size {
        for c in 0..size {
            full[r + quiet_zone][c + quiet_zone] = matrix[r][c];
        }
    }

    let (both_dark, top_dark, bot_dark, both_light) = if invert {
        (' ', '▄', '▀', '█')
    } else {
        ('█', '▀', '▄', ' ')
    };

    let mut lines = Vec::new();
    let mut r = 0;
    while r < full_size {
        let mut line = String::with_capacity(full_size);
        for (c, _) in full[0].iter().enumerate() {
            let top = if r < full_size { full[r][c] } else { 0 };
            let bot = if r + 1 < full_size { full[r + 1][c] } else { 0 };
            let ch = if top != 0 && bot != 0 {
                both_dark
            } else if top != 0 {
                top_dark
            } else if bot != 0 {
                bot_dark
            } else {
                both_light
            };
            line.push(ch);
        }
        lines.push(line);
        r += 2;
    }

    Ok(lines.join("\n"))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gf_tables_consistency() {
        let gf = GfTables::new();
        for i in 0..255 {
            assert_eq!(gf.log[gf.exp[i] as usize] as usize, i);
        }
    }

    #[test]
    fn test_select_version_short() {
        assert_eq!(select_version("https://x.co").unwrap(), 1);
    }

    #[test]
    fn test_select_version_target_url() {
        let url =
            "https://auth.anaconda.com/ui/session/continue/abcdefgh-ijkl-mnop-qrst-uvwxyzabcdef";
        assert_eq!(url.len(), 82);
        assert_eq!(select_version(url).unwrap(), 5);
    }

    #[test]
    fn test_select_version_too_long() {
        let url = format!("https://example.com/{}", "x".repeat(150));
        assert!(matches!(select_version(&url), Err(QrError::TooLong(_))));
    }

    #[test]
    fn test_matrix_size_v1() {
        let matrix = generate_qr("https://x.co").unwrap();
        assert_eq!(matrix.len(), 21);
        assert!(matrix.iter().all(|row| row.len() == 21));
    }

    #[test]
    fn test_terminal_returns_string() {
        let result = qr_to_terminal("https://example.com", 4, false).unwrap();
        assert!(!result.is_empty());
    }

    #[test]
    fn test_top_left_finder() {
        let matrix = generate_qr("https://example.com").unwrap();
        let expected = [
            [1, 1, 1, 1, 1, 1, 1],
            [1, 0, 0, 0, 0, 0, 1],
            [1, 0, 1, 1, 1, 0, 1],
            [1, 0, 1, 1, 1, 0, 1],
            [1, 0, 1, 1, 1, 0, 1],
            [1, 0, 0, 0, 0, 0, 1],
            [1, 1, 1, 1, 1, 1, 1],
        ];
        for r in 0..7 {
            for c in 0..7 {
                assert_eq!(matrix[r][c], expected[r][c], "Finder mismatch at ({r},{c})");
            }
        }
    }
}
