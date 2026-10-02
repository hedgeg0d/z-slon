//! Slider attacks via fancy magic bitboards.
//!
//! Tables are built once on first use. Magics are found with a fixed-seed
//! PRNG, so every run derives identical tables; tests verify every lookup
//! against the reference ray-walk generators.

use lazy_static::lazy_static;

const ROOK_DIRS: [(i8, i8); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const BISHOP_DIRS: [(i8, i8); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// Reference ray-walk slider attacks (used to fill and verify the tables).
fn ray_attacks(sq: u8, occupied: u64, dirs: &[(i8, i8); 4]) -> u64 {
    let from_rank = (sq / 8) as i8;
    let from_file = (sq % 8) as i8;
    let mut attacks = 0u64;
    for &(dr, df) in dirs {
        let mut r = from_rank + dr;
        let mut f = from_file + df;
        while r >= 0 && r < 8 && f >= 0 && f < 8 {
            let bit = 1u64 << (r * 8 + f);
            attacks |= bit;
            if occupied & bit != 0 {
                break;
            }
            r += dr;
            f += df;
        }
    }
    attacks
}

/// Relevant blocker mask: every ray square except the far edge one —
/// blockers on the edge never change the attack set.
fn blocker_mask(sq: u8, dirs: &[(i8, i8); 4]) -> u64 {
    let from_rank = (sq / 8) as i8;
    let from_file = (sq % 8) as i8;
    let mut mask = 0u64;
    for &(dr, df) in dirs {
        let mut r = from_rank + dr;
        let mut f = from_file + df;
        while r >= 0 && r < 8 && f >= 0 && f < 8 {
            if r + dr >= 0 && r + dr < 8 && f + df >= 0 && f + df < 8 {
                mask |= 1u64 << (r * 8 + f);
            }
            r += dr;
            f += df;
        }
    }
    mask
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Find a perfect magic for one square: sparse random candidates, reject
/// early when the top byte of `mask * magic` is not dense enough.
fn find_magic(
    mask: u64,
    occupancies: &[u64],
    reference: &[u64],
    bits: u8,
    rng: &mut u64,
) -> u64 {
    let size = occupancies.len();
    let shift = 64 - bits;
    let mut used = vec![0u64; size];
    let mut epoch = vec![0u32; size];
    let mut trial = 0u32;
    loop {
        trial += 1;
        let mut magic = splitmix64(rng);
        magic &= splitmix64(rng) & splitmix64(rng);
        if ((mask.wrapping_mul(magic)) >> 56).count_ones() < 6 {
            continue;
        }
        let mut ok = true;
        for i in 0..size {
            let idx = (occupancies[i].wrapping_mul(magic) >> shift) as usize;
            if epoch[idx] != trial {
                epoch[idx] = trial;
                used[idx] = reference[i];
            } else if used[idx] != reference[i] {
                ok = false;
                break;
            }
        }
        if ok {
            return magic;
        }
    }
}

struct Slider {
    table: Vec<u64>,
    magics: [u64; 64],
    offsets: [usize; 64],
    masks: [u64; 64],
    shifts: [u8; 64],
}

impl Slider {
    fn build(dirs: &[(i8, i8); 4], rng: &mut u64) -> Self {
        let mut table = Vec::new();
        let mut magics = [0u64; 64];
        let mut offsets = [0usize; 64];
        let mut masks = [0u64; 64];
        let mut shifts = [0u8; 64];

        for sq in 0..64u8 {
            let mask = blocker_mask(sq, dirs);
            let bits = mask.count_ones() as u8;
            let size = 1usize << bits;

            // enumerate every blocker subset with the carry-trick
            let mut occupancies = Vec::with_capacity(size);
            let mut reference = Vec::with_capacity(size);
            let mut subset = 0u64;
            loop {
                occupancies.push(subset);
                reference.push(ray_attacks(sq, subset, dirs));
                subset = subset.wrapping_sub(mask) & mask;
                if subset == 0 {
                    break;
                }
            }

            let magic = find_magic(mask, &occupancies, &reference, bits, rng);

            let sq_idx = sq as usize;
            offsets[sq_idx] = table.len();
            magics[sq_idx] = magic;
            masks[sq_idx] = mask;
            shifts[sq_idx] = 64 - bits;

            table.resize(table.len() + size, 0);
            for (i, &occ) in occupancies.iter().enumerate() {
                let idx = (occ.wrapping_mul(magic) >> shifts[sq_idx]) as usize;
                table[offsets[sq_idx] + idx] = reference[i];
            }
        }

        Self {
            table,
            magics,
            offsets,
            masks,
            shifts,
        }
    }

    #[inline(always)]
    fn attacks(&self, sq: usize, occupied: u64) -> u64 {
        let occ = occupied & self.masks[sq];
        let idx = occ.wrapping_mul(self.magics[sq]) >> self.shifts[sq];
        self.table[self.offsets[sq] + idx as usize]
    }
}

lazy_static! {
    static ref ROOK: Slider = Slider::build(&ROOK_DIRS, &mut 0x243F_6A88_85A3_08D3);
    static ref BISHOP: Slider = Slider::build(&BISHOP_DIRS, &mut 0x1319_8A2E_0370_7344);
}

#[inline]
pub(crate) fn rook_attacks(sq: u8, occupied: u64) -> u64 {
    ROOK.attacks(sq as usize, occupied)
}

#[inline]
pub(crate) fn bishop_attacks(sq: u8, occupied: u64) -> u64 {
    BISHOP.attacks(sq as usize, occupied)
}

#[cfg(test)]
pub(crate) fn rook_attacks_reference(sq: u8, occupied: u64) -> u64 {
    ray_attacks(sq, occupied, &ROOK_DIRS)
}

#[cfg(test)]
pub(crate) fn bishop_attacks_reference(sq: u8, occupied: u64) -> u64 {
    ray_attacks(sq, occupied, &BISHOP_DIRS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verify(fast: fn(u8, u64) -> u64, reference: fn(u8, u64) -> u64, dirs: &[(i8, i8); 4]) {
        let mut rng = 0xDEAD_BEEF_CAFE_F00Du64;
        for sq in 0..64u8 {
            // exhaustive: every reachable table entry is a blocker subset
            let mask = blocker_mask(sq, dirs);
            let mut subset = 0u64;
            loop {
                assert_eq!(
                    fast(sq, subset),
                    reference(sq, subset),
                    "sq={} subset={:#x}",
                    sq,
                    subset
                );
                subset = subset.wrapping_sub(mask) & mask;
                if subset == 0 {
                    break;
                }
            }
            // random full-board occupancies (bits outside the mask)
            for _ in 0..2000 {
                let occ = splitmix64(&mut rng);
                assert_eq!(fast(sq, occ), reference(sq, occ), "sq={} occ={:#x}", sq, occ);
            }
        }
    }

    #[test]
    fn rook_tables_match_reference() {
        verify(rook_attacks, rook_attacks_reference, &ROOK_DIRS);
    }

    #[test]
    fn bishop_tables_match_reference() {
        verify(bishop_attacks, bishop_attacks_reference, &BISHOP_DIRS);
    }
}
