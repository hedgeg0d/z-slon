use std::fmt;

use arrayvec::ArrayVec;

use crate::board::{Board, Piece};

pub type MoveList = ArrayVec<Move, 256>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    pub promotion: Option<Piece>,
}

const fn build_knight_attacks() -> [u64; 64] {
    let mut table = [0u64; 64];
    let deltas: [(i8, i8); 8] = [
        (-2, -1), (-2, 1), (-1, -2), (-1, 2),
        (1, -2), (1, 2), (2, -1), (2, 1),
    ];
    let mut sq = 0;
    while sq < 64 {
        let r = (sq / 8) as i8;
        let f = (sq % 8) as i8;
        let mut i = 0;
        while i < 8 {
            let (dr, df) = deltas[i];
            let nr = r + dr;
            let nf = f + df;
            if nr >= 0 && nr < 8 && nf >= 0 && nf < 8 {
                table[sq] |= 1u64 << (nr * 8 + nf);
            }
            i += 1;
        }
        sq += 1;
    }
    table
}

const fn build_king_attacks() -> [u64; 64] {
    let mut table = [0u64; 64];
    let mut sq = 0;
    while sq < 64 {
        let r = (sq / 8) as i8;
        let f = (sq % 8) as i8;
        let mut dr = -1i8;
        while dr <= 1 {
            let mut df = -1i8;
            while df <= 1 {
                if !(dr == 0 && df == 0) {
                    let nr = r + dr;
                    let nf = f + df;
                    if nr >= 0 && nr < 8 && nf >= 0 && nf < 8 {
                        table[sq] |= 1u64 << (nr * 8 + nf);
                    }
                }
                df += 1;
            }
            dr += 1;
        }
        sq += 1;
    }
    table
}

const fn build_pawn_attacks() -> [[u64; 64]; 2] {
    let mut table = [[0u64; 64]; 2];
    let mut sq = 0i32;
    while sq < 64 {
        let f = sq % 8;
        if sq + 7 < 64 && f > 0 {
            table[0][sq as usize] |= 1u64 << (sq + 7);
        }
        if sq + 9 < 64 && f < 7 {
            table[0][sq as usize] |= 1u64 << (sq + 9);
        }
        if sq - 7 >= 0 && f < 7 {
            table[1][sq as usize] |= 1u64 << (sq - 7);
        }
        if sq - 9 >= 0 && f > 0 {
            table[1][sq as usize] |= 1u64 << (sq - 9);
        }
        sq += 1;
    }
    table
}

pub(crate) static KNIGHT_ATTACKS: [u64; 64] = build_knight_attacks();
pub(crate) static KING_ATTACKS: [u64; 64] = build_king_attacks();
pub(crate) static PAWN_ATTACKS: [[u64; 64]; 2] = build_pawn_attacks();

fn ray_attacks(sq: u8, occupied: u64, directions: &[(i8, i8); 4]) -> u64 {
    let from_rank = (sq / 8) as i8;
    let from_file = (sq % 8) as i8;
    let mut attacks = 0u64;
    for &(dr, df) in directions {
        let mut r = from_rank + dr;
        let mut f = from_file + df;
        while r >= 0 && r < 8 && f >= 0 && f < 8 {
            let bit = 1u64 << (r * 8 + f);
            attacks |= bit;
            if (occupied & bit) != 0 {
                break;
            }
            r += dr;
            f += df;
        }
    }
    attacks
}

pub(crate) fn bishop_attacks(sq: u8, occupied: u64) -> u64 {
    ray_attacks(sq, occupied, &[(1, 1), (1, -1), (-1, 1), (-1, -1)])
}

pub(crate) fn rook_attacks(sq: u8, occupied: u64) -> u64 {
    ray_attacks(sq, occupied, &[(1, 0), (-1, 0), (0, 1), (0, -1)])
}

pub(crate) fn attackers_to(board: &Board, sq: u8, occupied: u64) -> u64 {
    let s = sq as usize;
    let diag_sliders = board.pieces[2] | board.pieces[4] | board.pieces[8] | board.pieces[10];
    let orth_sliders = board.pieces[3] | board.pieces[4] | board.pieces[9] | board.pieces[10];
    (PAWN_ATTACKS[1][s] & board.pieces[0])
        | (PAWN_ATTACKS[0][s] & board.pieces[6])
        | (KNIGHT_ATTACKS[s] & (board.pieces[1] | board.pieces[7]))
        | (KING_ATTACKS[s] & (board.pieces[5] | board.pieces[11]))
        | (bishop_attacks(sq, occupied) & diag_sliders)
        | (rook_attacks(sq, occupied) & orth_sliders)
}

impl fmt::Display for Move {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let files = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
        let from_file = files[(self.from % 8) as usize];
        let from_rank = (self.from / 8) + 1;
        let to_file = files[(self.to % 8) as usize];
        let to_rank = (self.to / 8) + 1;
        write!(f, "{}{}{}{}", from_file, from_rank, to_file, to_rank)?;
        if let Some(p) = self.promotion {
            let promo = match p {
                Piece::WQueen | Piece::BQueen => 'q',
                Piece::WRook | Piece::BRook => 'r',
                Piece::WBishop | Piece::BBishop => 'b',
                Piece::WKnight | Piece::BKnight => 'n',
                _ => '?',
            };
            write!(f, "{}", promo)?;
        }
        Ok(())
    }
}

pub fn generate_moves(board: &Board) -> MoveList {
    let white = board.is_white_to_move();
    let mut moves = MoveList::new();
    generate_pawn_moves(board, white, &mut moves);
    generate_knight_moves(board, white, &mut moves);
    generate_bishop_moves(board, white, &mut moves);
    generate_rook_moves(board, white, &mut moves);
    generate_queen_moves(board, white, &mut moves);
    generate_king_moves(board, white, &mut moves);
    moves
}

fn move_is_legal_slow(board: &Board, m: Move, white: bool) -> bool {
    let mut temp = board.clone();
    let moving = temp.square(m.from);
    let is_ep = temp.square(m.to) == Piece::Empty
        && matches!(moving, Piece::WPawn | Piece::BPawn)
        && {
            let d = (m.from as i8 - m.to as i8).abs();
            d == 7 || d == 9
        };
    make_move(&mut temp, m);
    if is_ep {
        let dir = if white { -8 } else { 8 };
        let cap_sq = (m.to as i8 + dir) as u8;
        temp.remove_piece(cap_sq);
    }
    let king_sq = temp.king_sq(white);
    !is_square_attacked(&temp, king_sq, !white)
}

pub fn filter_legal_moves(board: &Board, mut moves: MoveList) -> MoveList {
    let white = board.is_white_to_move();
    moves.retain(|m| move_is_legal_slow(board, *m, white));
    moves
}

fn is_ep_move(board: &Board, m: Move) -> bool {
    matches!(board.square(m.from), Piece::WPawn | Piece::BPawn)
        && board.square(m.to) == Piece::Empty
        && (m.from % 8) != (m.to % 8)
}

fn compute_pinned(board: &Board, white: bool, king_sq: u8) -> u64 {
    let kr = (king_sq / 8) as i8;
    let kf = (king_sq % 8) as i8;
    let mut pinned = 0u64;
    const DIRS: [(i8, i8, bool); 8] = [
        (1, 0, true), (-1, 0, true), (0, 1, true), (0, -1, true),
        (1, 1, false), (1, -1, false), (-1, 1, false), (-1, -1, false),
    ];
    for (dr, df, orth) in DIRS {
        let mut r = kr + dr;
        let mut f = kf + df;
        let mut blocker: Option<u8> = None;
        while r >= 0 && r < 8 && f >= 0 && f < 8 {
            let sq = (r * 8 + f) as u8;
            let piece = board.square(sq);
            if piece != Piece::Empty {
                if piece.is_white() == white {
                    if blocker.is_some() {
                        break;
                    }
                    blocker = Some(sq);
                } else {
                    if let Some(b) = blocker {
                        let is_pinner = if orth {
                            matches!(piece, Piece::WRook | Piece::BRook | Piece::WQueen | Piece::BQueen)
                        } else {
                            matches!(piece, Piece::WBishop | Piece::BBishop | Piece::WQueen | Piece::BQueen)
                        };
                        if is_pinner {
                            pinned |= 1u64 << b;
                        }
                    }
                    break;
                }
            }
            r += dr;
            f += df;
        }
    }
    pinned
}

fn filter_with_pins(board: &Board, mut pseudo: MoveList) -> MoveList {
    let white = board.is_white_to_move();
    let king_sq = board.king_sq(white);
    if king_sq >= 64 {
        return filter_legal_moves(board, pseudo);
    }
    let in_check = is_square_attacked(board, king_sq, !white);
    let pinned = compute_pinned(board, white, king_sq);
    pseudo.retain(|m| {
        let m = *m;
        let is_king = m.from == king_sq;
        let is_pinned = (pinned >> m.from) & 1 != 0;
        if !in_check && !is_king && !is_pinned && !is_ep_move(board, m) {
            true
        } else {
            move_is_legal_slow(board, m, white)
        }
    });
    pseudo
}

pub fn legal_moves(board: &Board) -> MoveList {
    filter_with_pins(board, generate_moves(board))
}

pub fn is_capture_move(board: &Board, m: Move) -> bool {
    let target = board.square(m.to);
    if target != Piece::Empty {
        return true;
    }
    let piece = board.square(m.from);
    if matches!(piece, Piece::WPawn | Piece::BPawn) {
        let diff = (m.from as i8 - m.to as i8).abs();
        if diff == 7 || diff == 9 {
            return board.en_passant.is_some();
        }
    }
    false
}

pub fn legal_captures(board: &Board) -> MoveList {
    let mut pseudo = generate_moves(board);
    pseudo.retain(|m| is_capture_move(board, *m));
    filter_with_pins(board, pseudo)
}

pub fn is_in_check(board: &Board, white: bool) -> bool {
    let king_sq = board.king_sq(white);
    is_square_attacked(board, king_sq, !white)
}

pub fn apply_move(board: &mut Board, mv: Move) {
    let moving_piece = board.square(mv.from);
    let moving_white = moving_piece.is_white();
    let mut captured_piece = board.square(mv.to);
    let diff = (mv.from as i8 - mv.to as i8).abs();

    let is_en_passant = captured_piece == Piece::Empty
        && matches!(moving_piece, Piece::WPawn | Piece::BPawn)
        && (diff == 7 || diff == 9);

    board.xor_castle_ep_zobrist();

    if is_en_passant {
        let dir = if moving_white { -8 } else { 8 };
        let captured_sq = (mv.to as i8 + dir) as u8;
        captured_piece = board.remove_piece(captured_sq);
    }

    let is_castling = matches!(moving_piece, Piece::WKing | Piece::BKing) && diff == 2;

    make_move(board, mv);

    if is_castling {
        let (rook_from, rook_to) = match mv.to {
            6 => (7, 5),
            2 => (0, 3),
            62 => (63, 61),
            58 => (56, 59),
            _ => unreachable!("Invalid castling move destination"),
        };
        let rook = board.remove_piece(rook_from);
        board.add_piece(rook_to, rook);
        if moving_white {
            board.castling &= !(1 | 2);
        } else {
            board.castling &= !(4 | 8);
        }
    } else {
        update_castling_rights(board, moving_piece, mv.from, mv.to, captured_piece);
    }

    if matches!(moving_piece, Piece::WPawn | Piece::BPawn) || captured_piece != Piece::Empty {
        board.halfmove = 0;
    } else {
        board.halfmove = board.halfmove.saturating_add(1);
    }

    if !moving_white {
        board.fullmove = board.fullmove.saturating_add(1);
    }

    board.xor_castle_ep_zobrist();
}

fn update_castling_rights(board: &mut Board, moving_piece: Piece, from: u8, to: u8, captured: Piece) {
    match moving_piece {
        Piece::WKing => board.castling &= !(1 | 2),
        Piece::BKing => board.castling &= !(4 | 8),
        Piece::WRook => {
            if from == 7 {
                board.castling &= !1;
            }
            if from == 0 {
                board.castling &= !2;
            }
        }
        Piece::BRook => {
            if from == 63 {
                board.castling &= !4;
            }
            if from == 56 {
                board.castling &= !8;
            }
        }
        _ => {}
    }

    match captured {
        Piece::WRook => {
            if to == 7 {
                board.castling &= !1;
            }
            if to == 0 {
                board.castling &= !2;
            }
        }
        Piece::BRook => {
            if to == 63 {
                board.castling &= !4;
            }
            if to == 56 {
                board.castling &= !8;
            }
        }
        _ => {}
    }
}

fn generate_pawn_moves(board: &Board, white: bool, moves: &mut MoveList) {
    let pawns = board.pieces[if white { 0 } else { 6 }];
    let direction = if white { 8 } else { -8 };
    let start_rank = if white { 1 } else { 6 };
    let promo_rank = if white { 7 } else { 0 };
    let occupied = board.pieces.iter().fold(0, |acc, &bb| acc | bb);
    let empty = !occupied;
    let ep_rank = if white { 4 } else { 3 };
    let mut bb = pawns;
    while bb != 0 {
        let from = bb.trailing_zeros() as u8;
        let to1 = (from as i8 + direction) as u8;
        if to1 < 64 && (empty & (1u64 << to1)) != 0 {
            if (to1 / 8) as usize == promo_rank {
                for &p in &[Piece::WQueen, Piece::WRook, Piece::WBishop, Piece::WKnight] {
                    let promo = if white {
                        p
                    } else {
                        match p {
                            Piece::WQueen => Piece::BQueen,
                            Piece::WRook => Piece::BRook,
                            Piece::WBishop => Piece::BBishop,
                            Piece::WKnight => Piece::BKnight,
                            _ => p,
                        }
                    };
                    moves.push(Move {
                        from,
                        to: to1,
                        promotion: Some(promo),
                    });
                }
            } else {
                moves.push(Move {
                    from,
                    to: to1,
                    promotion: None,
                });
            }
            let to2 = (from as i8 + direction * 2) as u8;
            if (from / 8) as usize == start_rank && to2 < 64 && (empty & (1u64 << to2)) != 0 {
                moves.push(Move {
                    from,
                    to: to2,
                    promotion: None,
                });
            }
        }
        for &d in &[-1, 1] {
            let to = (from as i8 + direction + d) as i8;
            if to >= 0 && to < 64 {
                let from_file = from % 8;
                let to_file = (to as u8) % 8;
                if (from_file as i8 - to_file as i8).abs() != 1 {
                    continue;
                }
                let to_u = to as u8;
                let target = board.square(to_u);
                if target != Piece::Empty && target.is_white() != white {
                    if (to_u / 8) as usize == promo_rank {
                        for &p in &[Piece::WQueen, Piece::WRook, Piece::WBishop, Piece::WKnight] {
                            let promo = if white {
                                p
                            } else {
                                match p {
                                    Piece::WQueen => Piece::BQueen,
                                    Piece::WRook => Piece::BRook,
                                    Piece::WBishop => Piece::BBishop,
                                    Piece::WKnight => Piece::BKnight,
                                    _ => p,
                                }
                            };
                            moves.push(Move {
                                from,
                                to: to_u,
                                promotion: Some(promo),
                            });
                        }
                    } else {
                        moves.push(Move {
                            from,
                            to: to_u,
                            promotion: None,
                        });
                    }
                }
            }
        }
        if let Some(ep_file) = board.en_passant {
            if (from / 8) as usize == ep_rank {
                let from_file = (from % 8) as i8;
                if (from_file - ep_file as i8).abs() == 1 {
                    let ep_target_rank = if white { 5 } else { 2 };
                    let to = ep_target_rank * 8 + ep_file;
                    moves.push(Move {
                        from,
                        to,
                        promotion: None,
                    });
                }
            }
        }
        bb &= bb - 1;
    }
}

fn generate_knight_moves(board: &Board, white: bool, moves: &mut MoveList) {
    let knights = board.pieces[if white { 1 } else { 7 }];
    let own_pieces = if white {
        board.pieces[0..6].iter().fold(0, |acc, &bb| acc | bb)
    } else {
        board.pieces[6..12].iter().fold(0, |acc, &bb| acc | bb)
    };
    let mut bb = knights;
    while bb != 0 {
        let from = bb.trailing_zeros() as u8;
        let mut attacks = KNIGHT_ATTACKS[from as usize] & !own_pieces;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as u8;
            moves.push(Move {
                from,
                to,
                promotion: None,
            });
            attacks &= attacks - 1;
        }
        bb &= bb - 1;
    }
}

fn generate_king_moves(board: &Board, white: bool, moves: &mut MoveList) {
    let kings = board.pieces[if white { 5 } else { 11 }];
    let own_pieces = if white {
        board.pieces[0..6].iter().fold(0, |acc, &bb| acc | bb)
    } else {
        board.pieces[6..12].iter().fold(0, |acc, &bb| acc | bb)
    };
    let all_pieces = board.pieces.iter().fold(0, |acc, &bb| acc | bb);
    let mut bb = kings;
    while bb != 0 {
        let from = bb.trailing_zeros() as u8;
        let mut attacks = KING_ATTACKS[from as usize] & !own_pieces;
        while attacks != 0 {
            let to = attacks.trailing_zeros() as u8;
            moves.push(Move {
                from,
                to,
                promotion: None,
            });
            attacks &= attacks - 1;
        }

        if white {
            if board.castling & 1 != 0
                && from == 4
                && (all_pieces & ((1u64 << 5) | (1u64 << 6))) == 0
                && !is_square_attacked(board, from, false)
                && !is_square_attacked(board, 5, false)
                && !is_square_attacked(board, 6, false)
            {
                moves.push(Move {
                    from,
                    to: 6,
                    promotion: None,
                });
            }
            if board.castling & 2 != 0
                && from == 4
                && (all_pieces & ((1u64 << 1) | (1u64 << 2) | (1u64 << 3))) == 0
                && !is_square_attacked(board, from, false)
                && !is_square_attacked(board, 3, false)
                && !is_square_attacked(board, 2, false)
            {
                moves.push(Move {
                    from,
                    to: 2,
                    promotion: None,
                });
            }
        } else {
            if board.castling & 4 != 0
                && from == 60
                && (all_pieces & ((1u64 << 61) | (1u64 << 62))) == 0
                && !is_square_attacked(board, from, true)
                && !is_square_attacked(board, 61, true)
                && !is_square_attacked(board, 62, true)
            {
                moves.push(Move {
                    from,
                    to: 62,
                    promotion: None,
                });
            }
            if board.castling & 8 != 0
                && from == 60
                && (all_pieces & ((1u64 << 57) | (1u64 << 58) | (1u64 << 59))) == 0
                && !is_square_attacked(board, from, true)
                && !is_square_attacked(board, 59, true)
                && !is_square_attacked(board, 58, true)
            {
                moves.push(Move {
                    from,
                    to: 58,
                    promotion: None,
                });
            }
        }
        bb &= bb - 1;
    }
}

fn generate_sliding_moves(board: &Board, white: bool, piece_idx: usize, directions: &[(i8, i8)], moves: &mut MoveList) {
    let pieces = board.pieces[piece_idx];
    let own_pieces = if white {
        board.pieces[0..6].iter().fold(0, |acc, &bb| acc | bb)
    } else {
        board.pieces[6..12].iter().fold(0, |acc, &bb| acc | bb)
    };
    let all_pieces = board.pieces.iter().fold(0, |acc, &bb| acc | bb);
    let mut bb = pieces;
    while bb != 0 {
        let from = bb.trailing_zeros() as u8;
        let from_rank = (from / 8) as i8;
        let from_file = (from % 8) as i8;
        for &(dr, df) in directions {
            let mut r = from_rank + dr;
            let mut f = from_file + df;
            while r >= 0 && r < 8 && f >= 0 && f < 8 {
                let to = (r * 8 + f) as u8;
                let to_mask = 1u64 << to;
                if (own_pieces & to_mask) != 0 {
                    break;
                }
                moves.push(Move {
                    from,
                    to,
                    promotion: None,
                });
                if (all_pieces & to_mask) != 0 {
                    break;
                }
                r += dr;
                f += df;
            }
        }
        bb &= bb - 1;
    }
}

fn generate_bishop_moves(board: &Board, white: bool, moves: &mut MoveList) {
    let piece_idx = if white { 2 } else { 8 };
    let directions = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
    generate_sliding_moves(board, white, piece_idx, &directions, moves)
}

fn generate_rook_moves(board: &Board, white: bool, moves: &mut MoveList) {
    let piece_idx = if white { 3 } else { 9 };
    let directions = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    generate_sliding_moves(board, white, piece_idx, &directions, moves)
}

fn generate_queen_moves(board: &Board, white: bool, moves: &mut MoveList) {
    let piece_idx = if white { 4 } else { 10 };
    let directions = [
        (1, 0),
        (-1, 0),
        (0, 1),
        (0, -1),
        (1, 1),
        (1, -1),
        (-1, 1),
        (-1, -1),
    ];
    generate_sliding_moves(board, white, piece_idx, &directions, moves)
}

fn is_square_attacked(board: &Board, sq: u8, by_white: bool) -> bool {
    let s = sq as usize;
    let base = if by_white { 0 } else { 6 };

    let pawn_table = if by_white { 1 } else { 0 };
    if PAWN_ATTACKS[pawn_table][s] & board.pieces[base] != 0 {
        return true;
    }
    if KNIGHT_ATTACKS[s] & board.pieces[base + 1] != 0 {
        return true;
    }
    if KING_ATTACKS[s] & board.pieces[base + 5] != 0 {
        return true;
    }

    let diag = board.pieces[base + 2] | board.pieces[base + 4];
    let orth = board.pieces[base + 3] | board.pieces[base + 4];
    if diag == 0 && orth == 0 {
        return false;
    }
    let all_pieces = board.pieces.iter().fold(0, |acc, &bb| acc | bb);
    if diag != 0 && bishop_attacks(sq, all_pieces) & diag != 0 {
        return true;
    }
    if orth != 0 && rook_attacks(sq, all_pieces) & orth != 0 {
        return true;
    }

    false
}

fn make_move(board: &mut Board, m: Move) {
    let piece = board.square(m.from);
    if piece == Piece::Empty {
        return;
    }
    board.remove_piece(m.to);
    board.remove_piece(m.from);
    let placed = m.promotion.unwrap_or(piece);
    board.add_piece(m.to, placed);
    if piece == Piece::WPawn || piece == Piece::BPawn {
        let rank_from = m.from / 8;
        let rank_to = m.to / 8;
        if (rank_from as i8 - rank_to as i8).abs() == 2 {
            board.en_passant = Some(m.from % 8);
        } else {
            board.en_passant = None;
        }
    } else {
        board.en_passant = None;
    }
    board.toggle_side();
}


#[cfg(test)]
mod perft_tests {
    use super::*;
    use crate::board::Board;

    fn perft(board: &Board, depth: u32) -> u64 {
        if depth == 0 {
            return 1;
        }
        let moves = legal_moves(board);
        if depth == 1 {
            return moves.len() as u64;
        }
        let mut nodes = 0;
        for mv in moves {
            let mut b = board.clone();
            apply_move(&mut b, mv);
            nodes += perft(&b, depth - 1);
        }
        nodes
    }

    fn check(fen: &str, depth: u32, expected: u64) {
        let board = Board::from_fen(fen);
        let got = perft(&board, depth);
        assert_eq!(got, expected, "perft({}) fen={} got {} expected {}", depth, fen, got, expected);
    }

    fn legality_dfs(board: &Board, depth: u32) {
        if depth == 0 {
            return;
        }
        let mover = board.is_white_to_move();
        for mv in legal_moves(board) {
            let mut b = board.clone();
            apply_move(&mut b, mv);
            assert!(
                b.king_sq(mover) < 64,
                "legal move {:?} left mover kingless: fen={}",
                mv,
                board.to_fen()
            );
            assert!(
                !is_in_check(&b, mover),
                "ILLEGAL move {} from {} -> mover king attacked after move",
                format_move(mv),
                board.to_fen()
            );
            legality_dfs(&b, depth - 1);
        }
    }

    fn format_move(m: Move) -> String {
        let f = |s: u8| format!("{}{}", (b'a' + s % 8) as char, (b'1' + s / 8) as char);
        format!("{}{}", f(m.from), f(m.to))
    }

    #[test]
    fn legality_random_playouts() {
        let mut state = 0xDEADBEEFCAFEu64;
        let mut rng = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for _ in 0..3000 {
            let mut board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
            let mut line: Vec<String> = Vec::new();
            for _ in 0..120 {
                let mover = board.is_white_to_move();
                let moves = legal_moves(&board);
                if moves.is_empty() {
                    break;
                }
                let mv = moves[(rng() as usize) % moves.len()];
                let prev = board.to_fen();
                apply_move(&mut board, mv);
                line.push(format_move(mv));
                assert!(
                    board.king_sq(mover) < 64 && !is_in_check(&board, mover),
                    "ILLEGAL {} from fen={} | line={:?}",
                    format_move(mv),
                    prev,
                    line
                );
            }
        }
    }

    fn zobrist_dfs(board: &Board, depth: u32) {
        if depth == 0 {
            return;
        }
        for mv in legal_moves(board) {
            let mut b = board.clone();
            apply_move(&mut b, mv);
            assert_eq!(
                b.zobrist,
                b.compute_zobrist(),
                "zobrist mismatch after {} from fen={}",
                format_move(mv),
                board.to_fen()
            );
            for sq in 0..64u8 {
                let bb_piece = (0..12)
                    .find(|&i| b.pieces[i] & (1u64 << sq) != 0)
                    .map(|i| i as usize);
                let mb = b.mailbox[sq as usize];
                let mb_idx = if mb == Piece::Empty { None } else { Some(mb as usize) };
                assert_eq!(bb_piece, mb_idx, "mailbox mismatch sq {} fen={}", sq, b.to_fen());
            }
            zobrist_dfs(&b, depth - 1);
        }
    }

    fn equiv_dfs(board: &Board, depth: u32) {
        let fast = legal_moves(board);
        let slow = filter_legal_moves(board, generate_moves(board));
        assert_eq!(
            fast, slow,
            "legal_moves mismatch fen={}\nfast={:?}\nslow={:?}",
            board.to_fen(), fast, slow
        );
        let fast_caps = legal_captures(board);
        let slow_caps = filter_legal_moves(
            board,
            generate_moves(board).into_iter().filter(|&m| is_capture_move(board, m)).collect(),
        );
        assert_eq!(fast_caps, slow_caps, "legal_captures mismatch fen={}", board.to_fen());
        if depth == 0 {
            return;
        }
        for mv in slow {
            let mut b = board.clone();
            apply_move(&mut b, mv);
            equiv_dfs(&b, depth - 1);
        }
    }

    #[test]
    fn legal_moves_pin_equivalence() {
        equiv_dfs(&Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"), 4);
        equiv_dfs(&Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"), 4);
        equiv_dfs(&Board::from_fen("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"), 5);
        equiv_dfs(&Board::from_fen("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1"), 4);
        equiv_dfs(&Board::from_fen("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8"), 4);
    }

    #[test]
    fn zobrist_consistency() {
        zobrist_dfs(&Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"), 4);
        zobrist_dfs(&Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"), 3);
        zobrist_dfs(&Board::from_fen("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1"), 3);
    }

    #[test]
    fn legality_startpos() {
        legality_dfs(&Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"), 4);
    }

    #[test]
    fn legality_kiwipete() {
        legality_dfs(&Board::from_fen("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1"), 4);
    }

    #[test]
    fn legality_pos3() {
        legality_dfs(&Board::from_fen("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1"), 5);
    }

    #[test]
    fn legality_pos4() {
        legality_dfs(&Board::from_fen("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1"), 4);
    }

    #[test]
    fn legality_pos5() {
        legality_dfs(&Board::from_fen("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8"), 4);
    }

    #[test]
    fn perft_startpos() {
        check("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", 5, 4865609);
    }
    #[test]
    fn perft_kiwipete() {
        check("r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1", 3, 97862);
    }
    #[test]
    fn perft_pos3() {
        check("8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1", 6, 11030083);
    }
    #[test]
    fn perft_pos4() {
        check("r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1", 3, 9467);
    }
    #[test]
    fn perft_pos5() {
        check("rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8", 3, 62379);
    }
}

#[cfg(test)]
mod null_move_tests {
    use super::*;
    use crate::board::Board;

    #[test]
    fn null_move_clears_en_passant() {
        let mut board = Board::from_fen("rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3 0 1");
        assert!(board.en_passant_file().is_some());
        board.make_null_move();
        assert!(board.en_passant_file().is_none());
        assert!(board.is_white_to_move());
        assert_eq!(board.position_hash(), board.compute_zobrist());
    }

    #[test]
    fn no_phantom_ep_after_null() {
        let mut board = Board::from_fen("4k3/8/8/3P4/8/8/8/4K3 b - e3 0 1");
        board.make_null_move();
        for m in legal_moves(&board) {
            let mut child = board.clone();
            apply_move(&mut child, m);
            assert!(child.king_sq(true) < 64 && child.king_sq(false) < 64);
        }
        assert!(!legal_moves(&board)
            .iter()
            .any(|m| m.from == 35 && m.to == 44));
    }
}
