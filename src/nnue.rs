use std::sync::{Arc, RwLock};
use crate::board::Board;
use timecat::evaluate::EvaluatorNNUE;
use timecat::Board as TimecatBoard;

pub struct NnueEvaluator {
    enabled: Arc<RwLock<bool>>,
    path: Arc<RwLock<Option<String>>>,
}

impl NnueEvaluator {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(RwLock::new(false)),
            path: Arc::new(RwLock::new(None)),
        }
    }

    pub fn load(&self, path: &str) -> Result<(), String> {
        // Timecat has built-in NNUE
        *self.enabled.write().unwrap() = true;
        *self.path.write().unwrap() = Some(path.to_string());
        Ok(())
    }

    pub fn is_loaded(&self) -> bool {
        *self.enabled.read().unwrap()
    }

    pub fn path(&self) -> Option<String> {
        self.path.read().unwrap().clone()
    }

    pub fn evaluate(&self, board: &Board) -> Option<i32> {
        if !self.is_loaded() {
            return None;
        }
        
        // Convert our board to timecat board via FEN
        let fen = board.to_fen();
        let timecat_board = TimecatBoard::from_fen(&fen)
            .map_err(|_| ()).ok()?;
        
        // Create position from board
        let position = timecat_board.get_position();
        
        // Evaluate using NNUE (returns i16, convert to i32)
        let score = EvaluatorNNUE::slow_evaluate(position) as i32;
        
        Some(score)
    }

    pub fn clone_handle(&self) -> Self {
        Self {
            enabled: Arc::clone(&self.enabled),
            path: Arc::clone(&self.path),
        }
    }
}

impl Clone for NnueEvaluator {
    fn clone(&self) -> Self {
        self.clone_handle()
    }
}
