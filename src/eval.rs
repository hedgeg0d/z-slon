use crate::board::{Board, Piece};
use crate::movegen::legal_moves;
use crate::nnue::NnueEvaluator;

pub struct EvaluationBreakdown {
    pub total: i32,
    pub material: i32,
    pub pst: i32,
    pub center: i32,
    pub mobility: i32,
    pub king_safety: i32,
}

const PAWN_PST_WHITE: [i32; 64] = [
    0, 0, 0, 0, 0, 0, 0, 0,
    5, 10, 10, -20, -20, 10, 10, 5,
    5, -5, -10, 0, 0, -10, -5, 5,
    0, 0, 0, 20, 20, 0, 0, 0,
    5, 5, 10, 25, 25, 10, 5, 5,
    10, 10, 20, 30, 30, 20, 10, 10,
    50, 50, 50, 50, 50, 50, 50, 50,
    0, 0, 0, 0, 0, 0, 0, 0,
];

const KNIGHT_PST_WHITE: [i32; 64] = [
    -50, -40, -30, -30, -30, -30, -40, -50,
    -40, -20, 0, 5, 5, 0, -20, -40,
    -30, 5, 10, 15, 15, 10, 5, -30,
    -30, 0, 15, 20, 20, 15, 0, -30,
    -30, 5, 15, 20, 20, 15, 5, -30,
    -30, 0, 10, 15, 15, 10, 0, -30,
    -40, -20, 0, 0, 0, 0, -20, -40,
    -50, -40, -30, -30, -30, -30, -40, -50,
];

const CENTER_SQUARES: [u8; 4] = [27, 28, 35, 36];
const CENTER_BONUS: i32 = 10;
const MOBILITY_FACTOR: i32 = 5;
const OPEN_FILE_PENALTY: i32 = 20;
const CASTLED_BONUS: i32 = 10;

pub fn evaluate(board: &Board) -> EvaluationBreakdown {
    evaluate_hce(board)
}

/// Evaluate using NNUE if available, otherwise fall back to HCE
pub fn evaluate_with_nnue(board: &Board, nnue: &Option<NnueEvaluator>) -> i32 {
    if let Some(evaluator) = nnue {
        if let Some(score) = evaluator.evaluate(board) {
            return score;
        }
    }
    // Fallback to HCE
    evaluate_hce(board).total
}

/// Hand-Crafted Evaluation (HCE) - the original evaluation function
pub fn evaluate_hce(board: &Board) -> EvaluationBreakdown {
    let mut material = 0;
    let mut pst = 0;
    let mut center = 0;

    for sq in 0..64 {
        let piece = board.square(sq as u8);
        if piece == Piece::Empty {
            continue;
        }
        material += piece_value(piece);
        pst += pst_value(piece, sq as usize);
        if CENTER_SQUARES.contains(&(sq as u8)) {
            if piece.is_white() {
                center += CENTER_BONUS;
            } else {
                center -= CENTER_BONUS;
            }
        }
    }

    let mobility = mobility_score(board);
    let king_safety = king_safety_score(board);
    let total = material + pst + center + mobility + king_safety;

    EvaluationBreakdown {
        total,
        material,
        pst,
        center,
        mobility,
        king_safety,
    }
}

fn piece_value(piece: Piece) -> i32 {
    match piece {
        Piece::WPawn => 100,
        Piece::WKnight => 320,
        Piece::WBishop => 330,
        Piece::WRook => 500,
        Piece::WQueen => 900,
        Piece::WKing => 0,
        Piece::BPawn => -100,
        Piece::BKnight => -320,
        Piece::BBishop => -330,
        Piece::BRook => -500,
        Piece::BQueen => -900,
        Piece::BKing => 0,
        Piece::Empty => 0,
    }
}

fn pst_value(piece: Piece, square: usize) -> i32 {
    match piece {
        Piece::WPawn => PAWN_PST_WHITE[square],
        Piece::BPawn => -PAWN_PST_WHITE[mirror_square(square)],
        Piece::WKnight => KNIGHT_PST_WHITE[square],
        Piece::BKnight => -KNIGHT_PST_WHITE[mirror_square(square)],
        _ => 0,
    }
}

fn mirror_square(square: usize) -> usize {
    square ^ 56
}

fn mobility_score(board: &Board) -> i32 {
    let mut white_board = board.clone();
    white_board.white_to_move = true;
    let white_moves = legal_moves(&white_board).len() as i32;

    let mut black_board = board.clone();
    black_board.white_to_move = false;
    let black_moves = legal_moves(&black_board).len() as i32;

    (white_moves - black_moves) * MOBILITY_FACTOR
}

fn king_safety_score(board: &Board) -> i32 {
    let mut score = 0;

    let white_king_sq = board.king_sq(true);
    let white_king_file = white_king_sq % 8;
    if !has_pawn_on_file(board, white_king_file, true) {
        score -= OPEN_FILE_PENALTY;
    }
    if white_king_sq == 6 || white_king_sq == 2 {
        score += CASTLED_BONUS;
    }

    let black_king_sq = board.king_sq(false);
    let black_king_file = black_king_sq % 8;
    if !has_pawn_on_file(board, black_king_file, false) {
        score += OPEN_FILE_PENALTY;
    }
    if black_king_sq == 62 || black_king_sq == 58 {
        score -= CASTLED_BONUS;
    }

    score
}

fn has_pawn_on_file(board: &Board, file: u8, white: bool) -> bool {
    for rank in 0..8 {
        let sq = rank * 8 + file;
        let piece = board.square(sq as u8);
        if white && piece == Piece::WPawn {
            return true;
        }
        if !white && piece == Piece::BPawn {
            return true;
        }
    }
    false
}
