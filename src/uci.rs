use std::io::{self, BufRead};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::board::Board;
use crate::movegen::{apply_move, legal_moves, Move};
use crate::search::search_position;
use crate::nnue::NnueEvaluator;
use crate::polyglot_integration::OptimizedPolyglotBook;
use tokio::sync::mpsc;

pub struct UciEngine {
    board: Board,
    depth: AtomicU32,
    threads: AtomicU32,
    hash_size: AtomicU32,
    searching: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    position_history: Vec<u64>,
    nnue: NnueEvaluator,
    book: Option<OptimizedPolyglotBook>,
    nnue_file: Option<String>,
}

impl UciEngine {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            board: Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"),
            depth: AtomicU32::new(20),
            threads: AtomicU32::new(1),
            hash_size: AtomicU32::new(16),
            searching: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            position_history: Vec::new(),
            nnue: NnueEvaluator::new(),
            book: None,
            nnue_file: None,
        }
    }
    
    pub fn new_with_nnue(nnue: NnueEvaluator) -> Self {
        Self {
            board: Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1"),
            depth: AtomicU32::new(20),
            threads: AtomicU32::new(1),
            hash_size: AtomicU32::new(16),
            searching: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            position_history: Vec::new(),
            nnue,
            book: None,
            nnue_file: None,
        }
    }
    
    pub fn set_threads(&mut self, threads: u32) {
        self.threads.store(threads.clamp(1, 64), Ordering::SeqCst);
    }
    
    pub fn load_book(&mut self, path: &str) {
        match OptimizedPolyglotBook::load(path) {
            Ok(book) => {
                println!("info string Polyglot book loaded: {}", path);
                let (entries, first_key, last_key) = book.debug_info();
                println!("info string Book entries: {}", entries);
                if let Some(first_key) = first_key {
                    println!("info string First entry key: 0x{:016X}", first_key);
                }
                if let Some(last_key) = last_key {
                    println!("info string Last entry key: 0x{:016X}", last_key);
                }
                self.book = Some(book);
            }
            Err(e) => {
                println!("info string Failed to load Polyglot book: {}", e);
            }
        }
    }
    
    fn is_draw_by_repetition(&self) -> bool {
        let hash = self.board.position_hash();
        self.position_history.iter().filter(|&&h| h == hash).count() >= 2
    }
    
    fn update_position_history(&mut self) {
        let hash = self.board.position_hash();
        self.position_history.push(hash);
    }
    
    fn clear_position_history(&mut self) {
        self.position_history.clear();
    }

    pub async fn run(&mut self) {
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        
        // Spawn input reader task
        tokio::spawn(async move {
            let stdin = io::stdin();
            let mut lines = stdin.lock().lines();
            while let Some(Ok(line)) = lines.next() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if tx.send(trimmed.to_string()).is_err() {
                        break;
                    }
                }
            }
        });

        // Process commands
        while let Some(command) = rx.recv().await {
            if !self.handle_command(&command).await {
                break;
            }
        }
    }

    async fn handle_command(&mut self, command: &str) -> bool {
        let parts: Vec<&str> = command.split_whitespace().collect();
        if parts.is_empty() {
            return true;
        }

        match parts[0] {
            "uci" => {
                println!("id name z-slon 0.2.0");
                println!("id author hedgegod");
                println!("option name Hash type spin default 16 min 1 max 33554432");
                println!("option name Threads type spin default 1 min 1 max 512");
                println!("option name UCI_Chess960 type check default false");
                println!("option name Ponder type check default false");
                println!("option name MultiPV type spin default 1 min 1 max 500");
                println!("option name Move Overhead type spin default 10 min 0 max 5000");
                println!("option name nodestime type spin default 0 min 0 max 10000");
                println!("option name EvalFile type string default <embedded>");
                println!("option name Book type string default <empty>");
                println!("uciok");
            }
            "isready" => {
                println!("readyok");
            }
            "ucinewgame" => {
                self.board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
                self.clear_position_history();
            }
            "position" => {
                self.handle_position(&parts[1..]);
            }
            "go" => {
                self.handle_go(&parts[1..]).await;
            }
            "stop" => {
                self.cancel_flag.store(true, Ordering::SeqCst);
            }
            "use" => {
                if parts.len() >= 3 && parts[1] == "nnue" {
                    self.handle_use_nnue(&parts[2..]);
                }
            }
            "setoption" => {
                self.handle_setoption(&parts[1..]);
            }
            "quit" => {
                self.cancel_flag.store(true, Ordering::SeqCst);
                return false;
            }
            _ => {}
        }
        true
    }

    fn handle_position(&mut self, parts: &[&str]) {
        if parts.is_empty() {
            return;
        }

        let mut idx = 0;
        
        // Parse position
        if parts[idx] == "startpos" {
            self.board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
            self.clear_position_history();
            idx += 1;
        } else if parts[idx] == "fen" {
            idx += 1;
            let mut fen_parts = Vec::new();
            while idx < parts.len() && parts[idx] != "moves" {
                fen_parts.push(parts[idx]);
                idx += 1;
            }
            let fen = fen_parts.join(" ");
            self.board = Board::from_fen(&fen);
            self.clear_position_history();
        }

        // Update history for initial position
        self.update_position_history();

        // Parse moves
        if idx < parts.len() && parts[idx] == "moves" {
            idx += 1;
            while idx < parts.len() {
                let move_str = parts[idx];
                if let Some(mv) = self.parse_uci_move(move_str) {
                    apply_move(&mut self.board, mv);
                    self.update_position_history();
                }
                idx += 1;
            }
        }
    }

    fn parse_uci_move(&self, move_str: &str) -> Option<Move> {
        if move_str.len() < 4 {
            return None;
        }

        let chars: Vec<char> = move_str.chars().collect();
        let from_file = (chars[0] as u8).wrapping_sub(b'a');
        let from_rank = (chars[1] as u8).wrapping_sub(b'1');
        let to_file = (chars[2] as u8).wrapping_sub(b'a');
        let to_rank = (chars[3] as u8).wrapping_sub(b'1');

        if from_file > 7 || from_rank > 7 || to_file > 7 || to_rank > 7 {
            return None;
        }

        let from = from_rank * 8 + from_file;
        let to = to_rank * 8 + to_file;

        let promotion = if chars.len() == 5 {
            let promo_char = chars[4];
            Some(self.promotion_piece(promo_char)?)
        } else {
            None
        };

        let legal = legal_moves(&self.board);
        legal.iter()
            .find(|m| m.from == from && m.to == to && m.promotion == promotion)
            .copied()
    }

    fn promotion_piece(&self, ch: char) -> Option<crate::board::Piece> {
        use crate::board::Piece;
        let white = self.board.is_white_to_move();
        match ch {
            'q' => Some(if white { Piece::WQueen } else { Piece::BQueen }),
            'r' => Some(if white { Piece::WRook } else { Piece::BRook }),
            'b' => Some(if white { Piece::WBishop } else { Piece::BBishop }),
            'n' => Some(if white { Piece::WKnight } else { Piece::BKnight }),
            _ => None,
        }
    }

    async fn handle_go(&mut self, parts: &[&str]) {
        // Check book first
        if let Some(ref book) = self.book {
            if let Some(book_move) = book.get_best_move(&self.board) {
                println!("info string Book move found");
                println!("bestmove {}", move_to_uci(book_move));
                return;
            }
        }

        let mut depth = self.depth.load(Ordering::SeqCst);
        let mut movetime = None;
        let mut wtime = None;
        let mut btime = None;
        let mut _winc = None;
        let mut _binc = None;
        let mut _infinite = false;

        let mut i = 0;
        while i < parts.len() {
            match parts[i] {
                "depth" => {
                    if i + 1 < parts.len() {
                        if let Ok(d) = parts[i + 1].parse::<u32>() {
                            depth = d;
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "movetime" => {
                    if i + 1 < parts.len() {
                        if let Ok(mt) = parts[i + 1].parse::<u64>() {
                            movetime = Some(mt);
                            // Set high depth for movetime mode
                            depth = 100;
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "wtime" => {
                    if i + 1 < parts.len() {
                        if let Ok(t) = parts[i + 1].parse::<u64>() {
                            wtime = Some(t);
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "btime" => {
                    if i + 1 < parts.len() {
                        if let Ok(t) = parts[i + 1].parse::<u64>() {
                            btime = Some(t);
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "winc" => {
                    if i + 1 < parts.len() {
                        if let Ok(inc) = parts[i + 1].parse::<u64>() {
                            _winc = Some(inc);
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "binc" => {
                    if i + 1 < parts.len() {
                        if let Ok(inc) = parts[i + 1].parse::<u64>() {
                            _binc = Some(inc);
                        }
                        i += 2;
                    } else {
                        i += 1;
                    }
                }
                "infinite" => {
                    _infinite = true;
                    depth = 100;
                    i += 1;
                }
                _ => {
                    i += 1;
                }
            }
        }

        // Calculate time to use based on clock
        if movetime.is_none() {
            let our_time = if self.board.is_white_to_move() { wtime } else { btime };
            if let Some(time_left) = our_time {
                // Simple time management: use 1/30 of remaining time
                // This is a basic heuristic, more sophisticated time management can be added
                let time_to_use = time_left / 30;
                movetime = Some(time_to_use.max(100)); // At least 100ms
                depth = 100; // Search deep but stop on time
            }
        }

        // Check for draw by repetition - but still need to return a legal move
        let is_repetition_draw = self.is_draw_by_repetition();
        if is_repetition_draw {
            println!("info string Draw by repetition");
        }

        self.searching.store(true, Ordering::SeqCst);
        self.cancel_flag.store(false, Ordering::SeqCst);

        let board_clone = self.board.clone();
        let threads = self.threads.load(Ordering::SeqCst);
        let cancel_clone = Arc::clone(&self.cancel_flag);
        let searching_clone = Arc::clone(&self.searching);

        let start_time = Instant::now();

        // Spawn time control task if movetime is set
        if let Some(mt) = movetime {
            let cancel_for_time = Arc::clone(&self.cancel_flag);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(mt)).await;
                cancel_for_time.store(true, Ordering::SeqCst);
            });
        }

        let nnue_clone = if self.nnue.is_loaded() {
            Some(self.nnue.clone_handle())
        } else {
            None
        };
        
        // Print evaluation mode info
        if self.nnue.is_loaded() {
            if let Some(path) = self.nnue.path() {
                println!("info string Using NNUE evaluation: {}", path);
            } else {
                println!("info string Using NNUE evaluation");
            }
        } else {
            println!("info string Using HCE evaluation");
        }
        
        let history = self.position_history.clone();
        
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = tokio::task::spawn_blocking(move || {
            search_position(board_clone, depth, threads, history, cancel_clone, nnue_clone, |event| {
                let _ = tx.send(event);
            })
        });

        // Spawn search task that runs independently
        tokio::spawn(async move {
            let mut best_move = None;
            let mut best_depth = 0;

            // Receive and print search info
            while let Some(event) = rx.recv().await {
                // Always use the move from the deepest completed search
                if let Some(mv) = event.best_move {
                    if event.depth >= best_depth {
                        best_move = Some(mv);
                        best_depth = event.depth;
                    }
                }
                let elapsed = start_time.elapsed().as_millis() as u64;
                
                print!("info depth {} seldepth {} score cp {} nodes {}", 
                    event.depth, event.seldepth, event.score, event.nodes);
                
                if elapsed > 0 {
                    let nps = (event.nodes as u64 * 1000) / elapsed;
                    print!(" time {} nps {}", elapsed, nps);
                }
                
                if event.pv_len > 0 {
                    print!(" pv");
                    for i in 0..event.pv_len {
                        if let Some(mv) = event.pv[i] {
                            print!(" {}", move_to_uci(mv));
                        }
                    }
                }
                
                println!();
            }

            let _ = handle.await;
            searching_clone.store(false, Ordering::SeqCst);

            // Send bestmove
            if let Some(mv) = best_move {
                println!("bestmove {}", move_to_uci(mv));
            } else {
                println!("bestmove 0000");
            }
        });
    }

    fn handle_setoption(&mut self, parts: &[&str]) {
        if parts.len() < 2 || parts[0] != "name" {
            return;
        }

        // Find "name" and "value" positions
        let name_start = 1;
        let mut name_end = name_start;
        let mut value_start = None;
        
        for (i, &part) in parts.iter().enumerate().skip(name_start) {
            if part == "value" {
                name_end = i;
                value_start = Some(i + 1);
                break;
            }
        }
        
        if name_end == name_start {
            name_end = parts.len();
        }
        
        let option_name = parts[name_start..name_end].join(" ").to_lowercase();
        
        if let Some(value_idx) = value_start {
            if value_idx < parts.len() {
                let value = parts[value_idx..].join(" ");
                
                match option_name.as_str() {
                    "threads" => {
                        if let Ok(t) = value.parse::<u32>() {
                            self.threads.store(t.clamp(1, 512), Ordering::SeqCst);
                            println!("info string Threads set to {}", t.clamp(1, 512));
                        }
                    }
                    "hash" => {
                        if let Ok(h) = value.parse::<u32>() {
                            self.hash_size.store(h.clamp(1, 33554432), Ordering::SeqCst);
                            println!("info string Hash size set to {} MB", h.clamp(1, 33554432));
                        }
                    }
                    "book" => {
                        if !value.is_empty() && value != "<empty>" {
                            self.load_book(&value);
                        }
                    }
                    "evalfile" => {
                        if value == "<embedded>" {
                            println!("info string Using embedded NNUE");
                        } else {
                            self.handle_evalfile_export(&value);
                        }
                    }
                    "uci_chess960" | "ponder" | "multipv" | "move overhead" | "nodestime" => {
                        println!("info string Option {} not yet implemented", option_name);
                    }
                    _ => {}
                }
            }
        }
    }
    
    fn handle_evalfile_export(&mut self, path: &str) {
        use crate::nnue::EMBEDDED_NNUE;
        
        match std::fs::write(path, EMBEDDED_NNUE) {
            Ok(_) => {
                println!("info string Exported embedded NNUE to: {}", path);
                self.nnue_file = Some(path.to_string());
            }
            Err(e) => {
                println!("info string Failed to export NNUE: {}", e);
            }
        }
    }
    
    fn handle_use_nnue(&mut self, parts: &[&str]) {
        if parts.is_empty() {
            println!("info string Error: missing NNUE path");
            return;
        }
        
        let path = parts.join(" ");
        match self.nnue.load(&path) {
            Ok(_) => {
                println!("info string NNUE loaded: {}", path);
                println!("info string Using NNUE evaluation");
            }
            Err(e) => {
                println!("info string Failed to load NNUE: {}", e);
            }
        }
    }
}

impl UciEngine {
    // FFI helper methods for Android bindings
    #[allow(dead_code)]
    pub fn set_position_from_fen(&mut self, fen: &str) {
        self.board = Board::from_fen(fen);
        self.clear_position_history();
        self.update_position_history();
    }
    
    #[allow(dead_code)]
    pub fn get_best_move(&self, _depth: u32) -> Option<String> {
        // Placeholder - real implementation would need async support
        None
    }
    
    #[allow(dead_code)]
    pub fn get_eval(&self) -> i32 {
        use crate::eval::evaluate;
        let breakdown = evaluate(&self.board);
        breakdown.total
    }
    
    #[allow(dead_code)]
    pub fn apply_move_uci(&mut self, move_str: &str) -> bool {
        if let Some(mv) = self.parse_uci_move(move_str) {
            apply_move(&mut self.board, mv);
            self.update_position_history();
            true
        } else {
            false
        }
    }
    
    #[allow(dead_code)]
    pub fn get_legal_moves(&self) -> Option<String> {
        let moves = legal_moves(&self.board);
        let moves_str = moves.iter()
            .map(|m| move_to_uci(*m))
            .collect::<Vec<_>>()
            .join(",");
        Some(moves_str)
    }
    
    #[allow(dead_code)]
    pub fn reset(&mut self) {
        self.board = Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1");
        self.clear_position_history();
        self.update_position_history();
    }
}

fn move_to_uci(mv: Move) -> String {
    let files = ['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h'];
    let from_file = files[(mv.from % 8) as usize];
    let from_rank = (mv.from / 8) + 1;
    let to_file = files[(mv.to % 8) as usize];
    let to_rank = (mv.to / 8) + 1;
    let mut s = format!("{}{}{}{}", from_file, from_rank, to_file, to_rank);
    
    if let Some(promo) = mv.promotion {
        use crate::board::Piece;
        let ch = match promo {
            Piece::WQueen | Piece::BQueen => 'q',
            Piece::WRook | Piece::BRook => 'r',
            Piece::WBishop | Piece::BBishop => 'b',
            Piece::WKnight | Piece::BKnight => 'n',
            _ => 'q',
        };
        s.push(ch);
    }
    s
}
