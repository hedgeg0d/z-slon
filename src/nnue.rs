use std::sync::Arc;

use nnue_rs::{Color as NnColor, Network, Piece as NnPiece, PieceKind};

use crate::board::Board;

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
        let white_relative = if board.is_white_to_move() {
            value
        } else {
            -value
        };
        Some(white_relative * 100 / NORMALIZE)
    }

    pub fn clone_handle(&self) -> Self {
        self.clone()
    }
}
