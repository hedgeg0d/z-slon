use std::sync::Arc;

use nnue_rs::{Accumulator, Color as NnColor, Network, Piece as NnPiece, PieceKind};

use crate::board::Board;

const SF_INDEX: [usize; 12] = [1, 2, 3, 4, 5, 6, 9, 10, 11, 12, 13, 14];

pub const EMBEDDED_NNUE: &[u8] = include_bytes!("../main_sf17.nnue");

const NORMALIZE: i32 = 380;

fn piece_for_index(index: usize) -> NnPiece {
    let (color, kind) = match index {
        0 => (NnColor::White, PieceKind::Pawn),
        1 => (NnColor::White, PieceKind::Knight),
        2 => (NnColor::White, PieceKind::Bishop),
        3 => (NnColor::White, PieceKind::Rook),
        4 => (NnColor::White, PieceKind::Queen),
        5 => (NnColor::White, PieceKind::King),
        6 => (NnColor::Black, PieceKind::Pawn),
        7 => (NnColor::Black, PieceKind::Knight),
        8 => (NnColor::Black, PieceKind::Bishop),
        9 => (NnColor::Black, PieceKind::Rook),
        10 => (NnColor::Black, PieceKind::Queen),
        _ => (NnColor::Black, PieceKind::King),
    };
    NnPiece::new(color, kind)
}

impl nnue_rs::Board for Board {
    fn side_to_move(&self) -> NnColor {
        if self.is_white_to_move() {
            NnColor::White
        } else {
            NnColor::Black
        }
    }

    fn king_square(&self, color: NnColor) -> u8 {
        self.king_sq(color == NnColor::White)
    }

    fn for_each_piece(&self, f: &mut dyn FnMut(u8, NnPiece)) {
        for index in 0..12 {
            let mut bb = self.pieces[index];
            let piece = piece_for_index(index);
            while bb != 0 {
                let sq = bb.trailing_zeros() as u8;
                f(sq, piece);
                bb &= bb - 1;
            }
        }
    }
}

#[derive(Clone)]
pub struct NnueEvaluator {
    network: Option<Arc<Network>>,
}

impl NnueEvaluator {
    pub fn new() -> Self {
        Self {
            network: Network::from_bytes(EMBEDDED_NNUE).ok().map(Arc::new),
        }
    }

    #[allow(dead_code)]
    pub fn new_disabled() -> Self {
        Self { network: None }
    }

    pub fn load(&self, _path: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        self.network.is_some()
    }

    pub fn path(&self) -> Option<String> {
        if self.network.is_some() {
            Some("<embedded sf17>".to_string())
        } else {
            None
        }
    }

    pub fn evaluate(&self, board: &Board) -> Option<i32> {
        let network = self.network.as_ref()?;
        if board.king_sq(true) >= 64 || board.king_sq(false) >= 64 {
            return None;
        }
        let value = network.evaluate(board);
        Some(self.normalize(value, board))
    }

    fn normalize(&self, value: i32, board: &Board) -> i32 {
        let white_relative = if board.is_white_to_move() { value } else { -value };
        white_relative * 100 / NORMALIZE
    }

    pub fn new_accumulator(&self) -> Option<Accumulator> {
        self.network.as_ref().map(|n| n.new_accumulator())
    }

    pub fn refresh(&self, board: &Board, acc: &mut Accumulator) {
        if let Some(n) = &self.network {
            n.refresh(board, acc);
        }
    }

    pub fn update(
        &self,
        parent_board: &Board,
        child_board: &Board,
        parent: &Accumulator,
        child: &mut Accumulator,
    ) {
        let network = match &self.network {
            Some(n) => n,
            None => return,
        };
        let white_king_moved = parent_board.pieces[5] != child_board.pieces[5];
        let black_king_moved = parent_board.pieces[11] != child_board.pieces[11];
        let mut removed = [(0u8, 0usize); 4];
        let mut added = [(0u8, 0usize); 4];
        let mut nr = 0;
        let mut na = 0;
        for i in 0..12 {
            let sf = SF_INDEX[i];
            let mut rem = parent_board.pieces[i] & !child_board.pieces[i];
            while rem != 0 {
                let sq = rem.trailing_zeros() as u8;
                if nr < removed.len() {
                    removed[nr] = (sq, sf);
                }
                nr += 1;
                rem &= rem - 1;
            }
            let mut add = child_board.pieces[i] & !parent_board.pieces[i];
            while add != 0 {
                let sq = add.trailing_zeros() as u8;
                if na < added.len() {
                    added[na] = (sq, sf);
                }
                na += 1;
                add &= add - 1;
            }
        }
        if nr > removed.len() || na > added.len() {
            network.refresh(child_board, child);
            return;
        }
        network.update(
            parent,
            child,
            child_board,
            white_king_moved,
            black_king_moved,
            &removed[..nr],
            &added[..na],
        );
    }

    pub fn eval_acc(&self, acc: &Accumulator, board: &Board) -> Option<i32> {
        let network = self.network.as_ref()?;
        if board.king_sq(true) >= 64 || board.king_sq(false) >= 64 {
            return None;
        }
        let piece_count = board.pieces.iter().map(|b| b.count_ones()).sum::<u32>() as usize;
        let stm = if board.is_white_to_move() {
            NnColor::White
        } else {
            NnColor::Black
        };
        let value = network.evaluate_accumulator(acc, stm, piece_count);
        Some(self.normalize(value, board))
    }

    pub fn clone_handle(&self) -> Self {
        self.clone()
    }
}

#[cfg(test)]
mod incremental_tests {
    use super::*;
    use crate::movegen::{apply_move, Move};
    use crate::board::Piece;

    fn sq(s: &str) -> u8 {
        let b = s.as_bytes();
        (b[0] - b'a') + (b[1] - b'1') * 8
    }

    fn check(parent_fen: &str, from: &str, to: &str, promo: Option<Piece>) {
        let ev = NnueEvaluator::new();
        assert!(ev.is_loaded());
        let parent = Board::from_fen(parent_fen);
        let mut parent_acc = ev.new_accumulator().unwrap();
        ev.refresh(&parent, &mut parent_acc);

        let mut child = parent.clone();
        apply_move(&mut child, Move { from: sq(from), to: sq(to), promotion: promo });

        let mut child_acc = ev.new_accumulator().unwrap();
        ev.update(&parent, &child, &parent_acc, &mut child_acc);

        let incremental = ev.eval_acc(&child_acc, &child).unwrap();
        let full = ev.evaluate(&child).unwrap();
        assert_eq!(incremental, full, "fen={} {}{}", parent_fen, from, to);
    }

    #[test]
    fn quiet_and_double_push() {
        check("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "e2", "e4", None);
        check("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1", "g1", "f3", None);
    }

    #[test]
    fn capture() {
        check("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq d6 0 2", "e4", "d5", None);
    }

    #[test]
    fn en_passant() {
        check("rnbqkbnr/ppp1p1pp/8/3pPp2/8/8/PPPP1PPP/RNBQKBNR w KQkq f6 0 3", "e5", "f6", None);
    }

    #[test]
    fn promotion() {
        check("4k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7", "a8", Some(Piece::WQueen));
    }

    #[test]
    fn promotion_capture() {
        check("1n2k3/P7/8/8/8/8/8/4K3 w - - 0 1", "a7", "b8", Some(Piece::WQueen));
    }

    #[test]
    fn castle_kingside() {
        check("r3k2r/8/8/8/8/8/8/R3K2R w KQkq - 0 1", "e1", "g1", None);
    }

    #[test]
    fn castle_queenside_black() {
        check("r3k2r/8/8/8/8/8/8/R3K2R b KQkq - 0 1", "e8", "c8", None);
    }

    #[test]
    fn king_move() {
        check("4k3/8/8/8/8/8/8/4K2R w K - 0 1", "e1", "e2", None);
    }

    #[test]
    fn king_capture() {
        check("4k3/8/8/8/8/8/3b4/4K3 w - - 0 1", "e1", "d2", None);
    }

    #[test]
    fn chain_multi_ply() {
        let ev = NnueEvaluator::new();
        let moves = [
            ("e2", "e4", None),
            ("c7", "c5", None),
            ("g1", "f3", None),
            ("d7", "d6", None),
            ("d2", "d4", None),
            ("c5", "d4", None),
            ("f3", "d4", None),
            ("g8", "f6", None),
            ("b1", "c3", None),
            ("e1", "g1", None),
        ];
        let mut board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let mut acc = ev.new_accumulator().unwrap();
        ev.refresh(&board, &mut acc);
        for (from, to, promo) in moves {
            let mut child = board.clone();
            apply_move(&mut child, Move { from: sq(from), to: sq(to), promotion: promo });
            let mut child_acc = ev.new_accumulator().unwrap();
            ev.update(&board, &child, &acc, &mut child_acc);
            let incremental = ev.eval_acc(&child_acc, &child).unwrap();
            let full = ev.evaluate(&child).unwrap();
            assert_eq!(incremental, full, "diverged after {}{}", from, to);
            board = child;
            acc = child_acc;
        }
    }
}
