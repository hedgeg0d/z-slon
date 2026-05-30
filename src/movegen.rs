use std::fmt;

use crate::board::{Board, Piece};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Move {
    pub from: u8,
    pub to: u8,
    pub promotion: Option<Piece>,
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

pub fn generate_moves(board: &Board) -> Vec<Move> {
    let white = board.is_white_to_move();
    let mut moves = Vec::new();
    moves.extend(generate_pawn_moves(board, white));
    moves.extend(generate_knight_moves(board, white));
    moves.extend(generate_bishop_moves(board, white));
    moves.extend(generate_rook_moves(board, white));
    moves.extend(generate_queen_moves(board, white));
    moves.extend(generate_king_moves(board, white));
    moves
}

pub fn filter_legal_moves(board: &Board, moves: Vec<Move>) -> Vec<Move> {
    let mut legal = Vec::new();
    let white = board.is_white_to_move();
    let old_ep = board.en_passant;
    for m in moves {
        let mut temp = board.clone();
        let captured_sq = if temp.en_passant.is_some()
            && temp.square(m.to) == Piece::Empty
            && (temp.square(m.from) == Piece::WPawn || temp.square(m.from) == Piece::BPawn)
            && (m.from as i8 - m.to as i8).abs() != 16
        {
            let dir = if white { -8 } else { 8 };
            (m.to as i8 + dir) as u8
        } else {
            m.to
        };
        let captured = temp.square(captured_sq);
        make_move(&mut temp, m);
        let new_king_sq = temp.king_sq(white);
        if !is_square_attacked(&temp, new_king_sq, !white) {
            legal.push(m);
        }
        unmake_move(&mut temp, m, captured, old_ep);
    }
    legal
}

pub fn legal_moves(board: &Board) -> Vec<Move> {
    let pseudo = generate_moves(board);
    filter_legal_moves(board, pseudo)
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

    if is_en_passant {
        let dir = if moving_white { -8 } else { 8 };
        let captured_sq = (mv.to as i8 + dir) as u8;
        captured_piece = board.square(captured_sq);
        if let Some(idx) = piece_to_index(captured_piece) {
            board.pieces[idx] &= !(1u64 << captured_sq);
        }
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
        let rook_idx = if moving_white { 3 } else { 9 };
        board.pieces[rook_idx] &= !(1u64 << rook_from);
        board.pieces[rook_idx] |= 1u64 << rook_to;
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
}

fn piece_to_index(piece: Piece) -> Option<usize> {
    match piece {
        Piece::WPawn => Some(0),
        Piece::WKnight => Some(1),
        Piece::WBishop => Some(2),
        Piece::WRook => Some(3),
        Piece::WQueen => Some(4),
        Piece::WKing => Some(5),
        Piece::BPawn => Some(6),
        Piece::BKnight => Some(7),
        Piece::BBishop => Some(8),
        Piece::BRook => Some(9),
        Piece::BQueen => Some(10),
        Piece::BKing => Some(11),
        Piece::Empty => None,
    }
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

fn generate_pawn_moves(board: &Board, white: bool) -> Vec<Move> {
    let mut moves = Vec::new();
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
    moves
}

fn generate_knight_moves(board: &Board, white: bool) -> Vec<Move> {
    let mut moves = Vec::new();
    let knights = board.pieces[if white { 1 } else { 7 }];
    let knight_deltas: [(i8, i8); 8] = [
        (-2, -1),
        (-2, 1),
        (-1, -2),
        (-1, 2),
        (1, -2),
        (1, 2),
        (2, -1),
        (2, 1),
    ];
    let mut bb = knights;
    while bb != 0 {
        let from = bb.trailing_zeros() as u8;
        let from_rank = (from / 8) as i8;
        let from_file = (from % 8) as i8;
        for &(dr, df) in &knight_deltas {
            let to_rank = from_rank + dr;
            let to_file = from_file + df;
            if to_rank >= 0 && to_rank < 8 && to_file >= 0 && to_file < 8 {
                let to_u = (to_rank * 8 + to_file) as u8;
                let target = board.square(to_u);
                if target == Piece::Empty || target.is_white() != white {
                    moves.push(Move {
                        from,
                        to: to_u,
                        promotion: None,
                    });
                }
            }
        }
        bb &= bb - 1;
    }
    moves
}

fn generate_king_moves(board: &Board, white: bool) -> Vec<Move> {
    let mut moves = Vec::new();
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
        let from_rank = from / 8;
        let from_file = from % 8;
        let mut attacks = 0u64;
        if from_rank > 0 {
            attacks |= 1u64 << (from - 8);
            if from_file > 0 {
                attacks |= 1u64 << (from - 9);
            }
            if from_file < 7 {
                attacks |= 1u64 << (from - 7);
            }
        }
        if from_rank < 7 {
            attacks |= 1u64 << (from + 8);
            if from_file > 0 {
                attacks |= 1u64 << (from + 7);
            }
            if from_file < 7 {
                attacks |= 1u64 << (from + 9);
            }
        }
        if from_file > 0 {
            attacks |= 1u64 << (from - 1);
        }
        if from_file < 7 {
            attacks |= 1u64 << (from + 1);
        }
        attacks &= !own_pieces;
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
    moves
}

fn generate_sliding_moves(board: &Board, white: bool, piece_idx: usize, directions: &[(i8, i8)]) -> Vec<Move> {
    let mut moves = Vec::new();
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
    moves
}

fn generate_bishop_moves(board: &Board, white: bool) -> Vec<Move> {
    let piece_idx = if white { 2 } else { 8 };
    let directions = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
    generate_sliding_moves(board, white, piece_idx, &directions)
}

fn generate_rook_moves(board: &Board, white: bool) -> Vec<Move> {
    let piece_idx = if white { 3 } else { 9 };
    let directions = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    generate_sliding_moves(board, white, piece_idx, &directions)
}

fn generate_queen_moves(board: &Board, white: bool) -> Vec<Move> {
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
    generate_sliding_moves(board, white, piece_idx, &directions)
}

fn is_square_attacked(board: &Board, sq: u8, by_white: bool) -> bool {
    let sq_rank = (sq / 8) as i8;
    let sq_file = (sq % 8) as i8;
    let all_pieces = board.pieces.iter().fold(0, |acc, &bb| acc | bb);

    let pawn_dir = if by_white { -8 } else { 8 };
    for &d in &[-1, 1] {
        let from = (sq as i8 + pawn_dir + d) as i8;
        if from >= 0 && from < 64 {
            let from_file = (from % 8) as i8;
            if (sq_file - from_file).abs() == 1 {
                let piece = board.square(from as u8);
                if piece == (if by_white { Piece::WPawn } else { Piece::BPawn }) {
                    return true;
                }
            }
        }
    }

    let knight_deltas: [(i8, i8); 8] = [
        (-2, -1),
        (-2, 1),
        (-1, -2),
        (-1, 2),
        (1, -2),
        (1, 2),
        (2, -1),
        (2, 1),
    ];
    for &(dr, df) in &knight_deltas {
        let from_rank = sq_rank + dr;
        let from_file = sq_file + df;
        if from_rank >= 0 && from_rank < 8 && from_file >= 0 && from_file < 8 {
            let from = (from_rank * 8 + from_file) as u8;
            let piece = board.square(from);
            if piece == (if by_white { Piece::WKnight } else { Piece::BKnight }) {
                return true;
            }
        }
    }

    let king_deltas: [(i8, i8); 8] = [
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (0, -1),
        (0, 1),
        (1, -1),
        (1, 0),
        (1, 1),
    ];
    for &(dr, df) in &king_deltas {
        let from_rank = sq_rank + dr;
        let from_file = sq_file + df;
        if from_rank >= 0 && from_rank < 8 && from_file >= 0 && from_file < 8 {
            let from = (from_rank * 8 + from_file) as u8;
            let piece = board.square(from);
            if piece == (if by_white { Piece::WKing } else { Piece::BKing }) {
                return true;
            }
        }
    }

    let bishop_directions = [(1, 1), (1, -1), (-1, 1), (-1, -1)];
    for &(dr, df) in &bishop_directions {
        let mut r = sq_rank + dr;
        let mut f = sq_file + df;
        while r >= 0 && r < 8 && f >= 0 && f < 8 {
            let from = (r * 8 + f) as u8;
            if (all_pieces & (1u64 << from)) != 0 {
                let piece = board.square(from);
                if piece == (if by_white { Piece::WBishop } else { Piece::BBishop })
                    || piece == (if by_white { Piece::WQueen } else { Piece::BQueen })
                {
                    return true;
                }
                break;
            }
            r += dr;
            f += df;
        }
    }

    let rook_directions = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    for &(dr, df) in &rook_directions {
        let mut r = sq_rank + dr;
        let mut f = sq_file + df;
        while r >= 0 && r < 8 && f >= 0 && f < 8 {
            let from = (r * 8 + f) as u8;
            if (all_pieces & (1u64 << from)) != 0 {
                let piece = board.square(from);
                if piece == (if by_white { Piece::WRook } else { Piece::BRook })
                    || piece == (if by_white { Piece::WQueen } else { Piece::BQueen })
                {
                    return true;
                }
                break;
            }
            r += dr;
            f += df;
        }
    }

    false
}

fn make_move(board: &mut Board, m: Move) {
    let piece = board.square(m.from);
    let from_mask = !(1u64 << m.from);
    let to_mask = !(1u64 << m.to);
    let idx = match piece {
        Piece::WPawn => 0,
        Piece::WKnight => 1,
        Piece::WBishop => 2,
        Piece::WRook => 3,
        Piece::WQueen => 4,
        Piece::WKing => 5,
        Piece::BPawn => 6,
        Piece::BKnight => 7,
        Piece::BBishop => 8,
        Piece::BRook => 9,
        Piece::BQueen => 10,
        Piece::BKing => 11,
        _ => return,
    };
    for bb in board.pieces.iter_mut() {
        *bb &= to_mask;
    }
    board.pieces[idx] &= from_mask;
    if let Some(promo) = m.promotion {
        let promo_idx = match promo {
            Piece::WQueen => 4,
            Piece::WRook => 3,
            Piece::WBishop => 2,
            Piece::WKnight => 1,
            Piece::BQueen => 10,
            Piece::BRook => 9,
            Piece::BBishop => 8,
            Piece::BKnight => 7,
            _ => return,
        };
        board.pieces[promo_idx] |= 1u64 << m.to;
    } else {
        board.pieces[idx] |= 1u64 << m.to;
    }
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
    board.white_to_move = !board.white_to_move;
}

fn unmake_move(board: &mut Board, m: Move, captured: Piece, old_ep: Option<u8>) {
    board.white_to_move = !board.white_to_move;
    let orig_piece = if m.promotion.is_some() {
        if board.white_to_move {
            Piece::WPawn
        } else {
            Piece::BPawn
        }
    } else {
        board.square(m.to)
    };
    let to_mask = !(1u64 << m.to);
    if let Some(promo) = m.promotion {
        let promo_idx = match promo {
            Piece::WQueen => 4,
            Piece::WRook => 3,
            Piece::WBishop => 2,
            Piece::WKnight => 1,
            Piece::BQueen => 10,
            Piece::BRook => 9,
            Piece::BBishop => 8,
            Piece::BKnight => 7,
            _ => return,
        };
        board.pieces[promo_idx] &= to_mask;
    } else {
        let idx = match orig_piece {
            Piece::WPawn => 0,
            Piece::WKnight => 1,
            Piece::WBishop => 2,
            Piece::WRook => 3,
            Piece::WQueen => 4,
            Piece::WKing => 5,
            Piece::BPawn => 6,
            Piece::BKnight => 7,
            Piece::BBishop => 8,
            Piece::BRook => 9,
            Piece::BQueen => 10,
            Piece::BKing => 11,
            _ => return,
        };
        board.pieces[idx] &= to_mask;
    }
    let orig_idx = match orig_piece {
        Piece::WPawn => 0,
        Piece::WKnight => 1,
        Piece::WBishop => 2,
        Piece::WRook => 3,
        Piece::WQueen => 4,
        Piece::WKing => 5,
        Piece::BPawn => 6,
        Piece::BKnight => 7,
        Piece::BBishop => 8,
        Piece::BRook => 9,
        Piece::BQueen => 10,
        Piece::BKing => 11,
        _ => return,
    };
    board.pieces[orig_idx] |= 1u64 << m.from;
    if captured != Piece::Empty {
        let cap_idx = match captured {
            Piece::WPawn => 0,
            Piece::WKnight => 1,
            Piece::WBishop => 2,
            Piece::WRook => 3,
            Piece::WQueen => 4,
            Piece::WKing => 5,
            Piece::BPawn => 6,
            Piece::BKnight => 7,
            Piece::BBishop => 8,
            Piece::BRook => 9,
            Piece::BQueen => 10,
            Piece::BKing => 11,
            _ => return,
        };
        board.pieces[cap_idx] |= 1u64 << m.to;
    }
    board.en_passant = old_ep;
}
