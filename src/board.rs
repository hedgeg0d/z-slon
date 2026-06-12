use std::fmt;
use std::hash::{Hash, Hasher};

use lazy_static::lazy_static;

pub type Bitboard = u64;

struct Zobrist {
    pieces: [[u64; 64]; 12],
    castle: [u64; 16],
    ep: [u64; 8],
    side: u64,
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

lazy_static! {
    static ref ZOBRIST: Zobrist = {
        let mut s = 0x1234_5678_9ABC_DEF0u64;
        let mut pieces = [[0u64; 64]; 12];
        for p in pieces.iter_mut() {
            for sq in p.iter_mut() {
                *sq = splitmix64(&mut s);
            }
        }
        let mut castle = [0u64; 16];
        for c in castle.iter_mut() {
            *c = splitmix64(&mut s);
        }
        let mut ep = [0u64; 8];
        for e in ep.iter_mut() {
            *e = splitmix64(&mut s);
        }
        let side = splitmix64(&mut s);
        Zobrist { pieces, castle, ep, side }
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    WPawn,
    WKnight,
    WBishop,
    WRook,
    WQueen,
    WKing,
    BPawn,
    BKnight,
    BBishop,
    BRook,
    BQueen,
    BKing,
    Empty,
}

impl Piece {
    pub fn from_char(c: char) -> Self {
        match c {
            'P' => Piece::WPawn,
            'N' => Piece::WKnight,
            'B' => Piece::WBishop,
            'R' => Piece::WRook,
            'Q' => Piece::WQueen,
            'K' => Piece::WKing,
            'p' => Piece::BPawn,
            'n' => Piece::BKnight,
            'b' => Piece::BBishop,
            'r' => Piece::BRook,
            'q' => Piece::BQueen,
            'k' => Piece::BKing,
            _ => Piece::Empty,
        }
    }

    pub fn to_char(&self) -> char {
        match self {
            Piece::WPawn => 'P',
            Piece::WKnight => 'N',
            Piece::WBishop => 'B',
            Piece::WRook => 'R',
            Piece::WQueen => 'Q',
            Piece::WKing => 'K',
            Piece::BPawn => 'p',
            Piece::BKnight => 'n',
            Piece::BBishop => 'b',
            Piece::BRook => 'r',
            Piece::BQueen => 'q',
            Piece::BKing => 'k',
            Piece::Empty => '.',
        }
    }

    pub fn is_white(&self) -> bool {
        matches!(self, Piece::WPawn | Piece::WKnight | Piece::WBishop | Piece::WRook | Piece::WQueen | Piece::WKing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board {
    pub(crate) pieces: [Bitboard; 12],
    pub(crate) mailbox: [Piece; 64],
    pub(crate) white_to_move: bool,
    pub(crate) castling: u8,
    pub(crate) en_passant: Option<u8>,
    pub(crate) halfmove: u8,
    pub(crate) fullmove: u16,
    pub(crate) zobrist: u64,
}

impl Hash for Board {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.pieces.hash(state);
        self.white_to_move.hash(state);
        self.castling.hash(state);
        self.en_passant.hash(state);
    }
}

impl Board {
    pub fn from_fen(fen: &str) -> Self {
        let mut board = Board {
            pieces: [0; 12],
            mailbox: [Piece::Empty; 64],
            white_to_move: true,
            castling: 0,
            en_passant: None,
            halfmove: 0,
            fullmove: 1,
            zobrist: 0,
        };
        let parts: Vec<&str> = fen.split_whitespace().collect();
        let mut row = 7;
        let mut col = 0;
        for c in parts[0].chars() {
            if c == '/' {
                row -= 1;
                col = 0;
            } else if c.is_digit(10) {
                col += c.to_digit(10).unwrap() as usize;
            } else {
                let piece = Piece::from_char(c);
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
                    _ => continue,
                };
                let sq = row * 8 + col;
                board.pieces[idx] |= 1u64 << sq;
                board.mailbox[sq] = piece;
                col += 1;
            }
        }
        board.white_to_move = parts[1] == "w";
        for c in parts[2].chars() {
            board.castling |= match c {
                'K' => 1,
                'Q' => 2,
                'k' => 4,
                'q' => 8,
                _ => 0,
            };
        }
        if parts[3] != "-" {
            let file = parts[3].chars().next().unwrap() as u8 - b'a';
            board.en_passant = Some(file);
        }
        if parts.len() > 4 {
            board.halfmove = parts[4].parse().unwrap_or(0);
        }
        if parts.len() > 5 {
            board.fullmove = parts[5].parse().unwrap_or(1);
        }
        board.zobrist = board.compute_zobrist();
        board
    }

    pub fn compute_zobrist(&self) -> u64 {
        let mut h = 0u64;
        for (i, &bb) in self.pieces.iter().enumerate() {
            let mut b = bb;
            while b != 0 {
                let sq = b.trailing_zeros() as usize;
                h ^= ZOBRIST.pieces[i][sq];
                b &= b - 1;
            }
        }
        h ^= ZOBRIST.castle[(self.castling & 15) as usize];
        if let Some(f) = self.en_passant {
            h ^= ZOBRIST.ep[(f & 7) as usize];
        }
        if self.white_to_move {
            h ^= ZOBRIST.side;
        }
        h
    }

    pub fn refresh_zobrist(&mut self) {
        self.zobrist = self.compute_zobrist();
    }

    pub fn toggle_side(&mut self) {
        self.white_to_move = !self.white_to_move;
        self.zobrist ^= ZOBRIST.side;
    }

    pub fn make_null_move(&mut self) {
        self.xor_castle_ep_zobrist();
        self.en_passant = None;
        self.xor_castle_ep_zobrist();
        self.toggle_side();
    }

    #[inline]
    pub(crate) fn add_piece(&mut self, sq: u8, piece: Piece) {
        let idx = piece as usize;
        self.pieces[idx] |= 1u64 << sq;
        self.mailbox[sq as usize] = piece;
        self.zobrist ^= ZOBRIST.pieces[idx][sq as usize];
    }

    #[inline]
    pub(crate) fn remove_piece(&mut self, sq: u8) -> Piece {
        let piece = self.mailbox[sq as usize];
        if piece != Piece::Empty {
            let idx = piece as usize;
            self.pieces[idx] &= !(1u64 << sq);
            self.mailbox[sq as usize] = Piece::Empty;
            self.zobrist ^= ZOBRIST.pieces[idx][sq as usize];
        }
        piece
    }

    #[inline]
    pub(crate) fn xor_castle_ep_zobrist(&mut self) {
        self.zobrist ^= ZOBRIST.castle[(self.castling & 15) as usize];
        if let Some(f) = self.en_passant {
            self.zobrist ^= ZOBRIST.ep[(f & 7) as usize];
        }
    }

    #[inline]
    pub fn square(&self, sq: u8) -> Piece {
        self.mailbox[sq as usize]
    }

    pub fn king_sq(&self, white: bool) -> u8 {
        let king_bb = self.pieces[if white { 5 } else { 11 }];
        king_bb.trailing_zeros() as u8
    }

    pub fn is_white_to_move(&self) -> bool {
        self.white_to_move
    }

    pub fn en_passant_file(&self) -> Option<u8> {
        self.en_passant
    }

    pub fn castling_rights(&self) -> u8 {
        self.castling
    }

    pub fn halfmove_clock(&self) -> u8 {
        self.halfmove
    }

    pub fn fullmove_number(&self) -> u16 {
        self.fullmove
    }

    pub fn to_fen(&self) -> String {
        let mut fen = String::new();
        for rank in (0..8).rev() {
            let mut empty_count = 0;
            for file in 0..8 {
                let sq = rank * 8 + file;
                let piece = self.square(sq);
                if piece == Piece::Empty {
                    empty_count += 1;
                } else {
                    if empty_count > 0 {
                        fen.push_str(&empty_count.to_string());
                        empty_count = 0;
                    }
                    fen.push(piece.to_char());
                }
            }
            if empty_count > 0 {
                fen.push_str(&empty_count.to_string());
            }
            if rank > 0 {
                fen.push('/');
            }
        }
        fen.push(' ');
        fen.push(if self.white_to_move { 'w' } else { 'b' });
        fen.push(' ');
        let mut castling = String::new();
        if self.castling & 1 != 0 {
            castling.push('K');
        }
        if self.castling & 2 != 0 {
            castling.push('Q');
        }
        if self.castling & 4 != 0 {
            castling.push('k');
        }
        if self.castling & 8 != 0 {
            castling.push('q');
        }
        if castling.is_empty() {
            castling.push('-');
        }
        fen.push_str(&castling);
        fen.push(' ');
        if let Some(file) = self.en_passant {
            let rank = if self.white_to_move { '6' } else { '3' };
            fen.push((b'a' + file) as char);
            fen.push(rank);
        } else {
            fen.push('-');
        }
        fen.push(' ');
        fen.push_str(&self.halfmove.to_string());
        fen.push(' ');
        fen.push_str(&self.fullmove.to_string());
        fen
    }

    pub fn position_hash(&self) -> u64 {
        self.zobrist
    }

    pub fn is_draw_by_fifty_move(&self) -> bool {
        self.halfmove >= 100
    }

    pub fn is_insufficient_material(&self) -> bool {
        let mut piece_count = 0;
        let mut has_pawn = false;
        let mut has_rook = false;
        let mut has_queen = false;
        let mut white_bishops = 0;
        let mut black_bishops = 0;
        let mut _white_knights = 0;
        let mut _black_knights = 0;

        for sq in 0..64 {
            let piece = self.square(sq);
            if piece != Piece::Empty && piece != Piece::WKing && piece != Piece::BKing {
                piece_count += 1;
                match piece {
                    Piece::WPawn | Piece::BPawn => has_pawn = true,
                    Piece::WRook | Piece::BRook => has_rook = true,
                    Piece::WQueen | Piece::BQueen => has_queen = true,
                    Piece::WBishop => white_bishops += 1,
                    Piece::BBishop => black_bishops += 1,
                    Piece::WKnight => _white_knights += 1,
                    Piece::BKnight => _black_knights += 1,
                    _ => {}
                }
            }
        }

        if piece_count == 0 {
            return true;
        }

        if piece_count == 1 && !has_pawn && !has_rook && !has_queen {
            return true;
        }

        if piece_count == 2 && white_bishops == 1 && black_bishops == 1 {
            return true;
        }

        false
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for row in (0..8).rev() {
            write!(f, "{} ", row + 1)?;
            for col in 0..8 {
                let sq = row * 8 + col;
                let piece = self.square(sq);
                write!(f, "{} ", piece.to_char())?;
            }
            writeln!(f)?;
        }
        writeln!(f, "  a b c d e f g h")
    }
}
