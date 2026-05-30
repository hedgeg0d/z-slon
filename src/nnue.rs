use std::sync::{Arc, RwLock};
use crate::board::Board;
use timecat::evaluate::EvaluatorNNUE;
use timecat::Board as TimecatBoard;

pub const EMBEDDED_NNUE: &[u8] = include_bytes!("../main_sf17.nnue");

pub struct NnueEvaluator {
    enabled: Arc<RwLock<bool>>,
    path: Arc<RwLock<Option<String>>>,
}

impl NnueEvaluator {
    pub fn new() -> Self {
        Self {
            enabled: Arc::new(RwLock::new(true)),
            path: Arc::new(RwLock::new(Some("<embedded>".to_string()))),
        }
    }
    
    #[allow(dead_code)]
    pub fn new_disabled() -> Self {
        Self {
            enabled: Arc::new(RwLock::new(false)),
            path: Arc::new(RwLock::new(None)),
        }
    }

    pub fn load(&self, path: &str) -> Result<(), String> {
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
        
        let fen = board.to_fen();
        let timecat_board = TimecatBoard::from_fen(&fen)
            .map_err(|_| ()).ok()?;
        
        let position = timecat_board.get_position();
        
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
