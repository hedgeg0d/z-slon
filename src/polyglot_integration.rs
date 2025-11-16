//! Integration with polyglot-rs crate for opening book support.

use crate::board::{Board, Piece as BoardPiece};
use crate::movegen::Move;

// Re-export from polyglot-book-rs crate
pub use polyglot_book_rs::{PolyglotBook, PolyglotMove};
use polyglot_book_rs::{BoardPosition, types::Piece as PolyglotPiece};

/// Implementation of BoardPosition trait for our Board struct.
/// This allows the polyglot-rs crate to work directly with our board representation.
impl BoardPosition for Board {
    fn piece_at(&self, square: u8) -> PolyglotPiece {
        let board_piece = self.square(square);
        convert_board_piece_to_polyglot(board_piece)
    }
    
    fn is_white_to_move(&self) -> bool {
        self.white_to_move
    }
    
    fn castling_rights(&self) -> u8 {
        self.castling
    }
    
    fn en_passant_file(&self) -> Option<u8> {
        self.en_passant
    }
}

/// Convert our internal Piece enum to polyglot-rs Piece enum.
fn convert_board_piece_to_polyglot(piece: BoardPiece) -> PolyglotPiece {
    match piece {
        BoardPiece::WPawn => PolyglotPiece::WPawn,
        BoardPiece::WKnight => PolyglotPiece::WKnight,
        BoardPiece::WBishop => PolyglotPiece::WBishop,
        BoardPiece::WRook => PolyglotPiece::WRook,
        BoardPiece::WQueen => PolyglotPiece::WQueen,
        BoardPiece::WKing => PolyglotPiece::WKing,
        BoardPiece::BPawn => PolyglotPiece::BPawn,
        BoardPiece::BKnight => PolyglotPiece::BKnight,
        BoardPiece::BBishop => PolyglotPiece::BBishop,
        BoardPiece::BRook => PolyglotPiece::BRook,
        BoardPiece::BQueen => PolyglotPiece::BQueen,
        BoardPiece::BKing => PolyglotPiece::BKing,
        BoardPiece::Empty => PolyglotPiece::Empty,
    }
}

/// Convert polyglot-rs Piece enum to our internal Piece enum.
fn convert_polyglot_piece_to_board(piece: PolyglotPiece) -> BoardPiece {
    match piece {
        PolyglotPiece::WPawn => BoardPiece::WPawn,
        PolyglotPiece::WKnight => BoardPiece::WKnight,
        PolyglotPiece::WBishop => BoardPiece::WBishop,
        PolyglotPiece::WRook => BoardPiece::WRook,
        PolyglotPiece::WQueen => BoardPiece::WQueen,
        PolyglotPiece::WKing => BoardPiece::WKing,
        PolyglotPiece::BPawn => BoardPiece::BPawn,
        PolyglotPiece::BKnight => BoardPiece::BKnight,
        PolyglotPiece::BBishop => BoardPiece::BBishop,
        PolyglotPiece::BRook => BoardPiece::BRook,
        PolyglotPiece::BQueen => BoardPiece::BQueen,
        PolyglotPiece::BKing => BoardPiece::BKing,
        PolyglotPiece::Empty => BoardPiece::Empty,
    }
}

/// Convert PolyglotMove to our internal Move struct.
pub fn convert_polyglot_move_to_move(polyglot_move: &PolyglotMove) -> Move {
    let promotion = polyglot_move.promotion.map(convert_polyglot_piece_to_board);
    Move {
        from: polyglot_move.from,
        to: polyglot_move.to,
        promotion,
    }
}

/// Enhanced PolyglotBook wrapper with optimized methods for our engine.
pub struct OptimizedPolyglotBook {
    book: PolyglotBook,
}

impl OptimizedPolyglotBook {
    /// Load a Polyglot book from file.
    pub fn load(path: &str) -> std::io::Result<Self> {
        let book = PolyglotBook::load(path)?;
        Ok(Self { book })
    }

    /// Get the best move for a board position.
    /// This is the most optimized method for your engine - no FEN conversion needed.
    pub fn get_best_move(&self, board: &Board) -> Option<Move> {
        self.book.get_best_move(board)
            .map(|entry| convert_polyglot_move_to_move(&entry.chess_move))
    }

    /// Get all moves for a position with their weights.
    /// Returns moves sorted by weight (highest first).
    pub fn get_all_moves(&self, board: &Board) -> Vec<(Move, u16)> {
        self.book.get_all_moves(board)
            .into_iter()
            .map(|entry| (convert_polyglot_move_to_move(&entry.chess_move), entry.weight))
            .collect()
    }

    /// Get the best move with its weight and position hash.
    pub fn get_best_move_with_info(&self, board: &Board) -> Option<(Move, u16, u64)> {
        self.book.get_best_move(board)
            .map(|entry| (
                convert_polyglot_move_to_move(&entry.chess_move),
                entry.weight,
                entry.position_hash
            ))
    }

    /// Check if a position exists in the book.
    pub fn has_position(&self, board: &Board) -> bool {
        self.book.has_position(board)
    }

    /// Get the number of entries in the book.
    pub fn entry_count(&self) -> usize {
        self.book.entry_count()
    }

    /// Get debug info about the book.
    pub fn debug_info(&self) -> (usize, Option<u64>, Option<u64>) {
        (
            self.book.entry_count(),
            self.book.first_key(),
            self.book.last_key(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_board_position_implementation() {
        let board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        
        // Test piece_at
        assert_eq!(board.piece_at(0), PolyglotPiece::WRook);   // a1
        assert_eq!(board.piece_at(4), PolyglotPiece::WKing);   // e1
        assert_eq!(board.piece_at(60), PolyglotPiece::BKing);  // e8
        assert_eq!(board.piece_at(63), PolyglotPiece::BRook);  // h8
        
        // Test other methods
        assert!(board.is_white_to_move());
        assert_eq!(board.castling_rights(), 15); // All castling rights
        assert_eq!(board.en_passant_file(), None);
    }

    #[test]
    fn test_piece_conversion() {
        // Test board piece to polyglot piece conversion
        assert_eq!(convert_board_piece_to_polyglot(BoardPiece::WPawn), PolyglotPiece::WPawn);
        assert_eq!(convert_board_piece_to_polyglot(BoardPiece::BQueen), PolyglotPiece::BQueen);
        assert_eq!(convert_board_piece_to_polyglot(BoardPiece::Empty), PolyglotPiece::Empty);
        
        // Test polyglot piece to board piece conversion
        assert_eq!(convert_polyglot_piece_to_board(PolyglotPiece::WKnight), BoardPiece::WKnight);
        assert_eq!(convert_polyglot_piece_to_board(PolyglotPiece::BRook), BoardPiece::BRook);
        assert_eq!(convert_polyglot_piece_to_board(PolyglotPiece::Empty), BoardPiece::Empty);
    }

    #[test]
    fn test_move_conversion() {
        let polyglot_move = PolyglotMove::new(12, 28, Some(PolyglotPiece::WQueen));
        let engine_move = convert_polyglot_move_to_move(&polyglot_move);
        
        assert_eq!(engine_move.from, 12);
        assert_eq!(engine_move.to, 28);
        assert_eq!(engine_move.promotion, Some(BoardPiece::WQueen));
    }

    #[test]
    fn test_hash_consistency() {
        // Test that our BoardPosition implementation produces the correct hash
        let board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        let hash = polyglot_book_rs::hash::polyglot_hash(&board);
        
        // This should match the known correct starting position hash
        assert_eq!(hash, 0x463B96181691FC9C);
    }
}
