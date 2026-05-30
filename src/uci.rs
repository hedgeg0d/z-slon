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
    multi_pv: AtomicU32,
    searching: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    pondering: Arc<AtomicBool>,
    ponder_enabled: AtomicBool,
    ponder_start_time: Option<Instant>,
    ponder_time_params: Option<(Option<u64>, Option<u64>, Option<u64>)>,
    expected_ponder_move: Option<Move>,
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
            multi_pv: AtomicU32::new(1),
            searching: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            pondering: Arc::new(AtomicBool::new(false)),
            ponder_enabled: AtomicBool::new(false),
            ponder_start_time: None,
            ponder_time_params: None,
            expected_ponder_move: None,
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
            multi_pv: AtomicU32::new(1),
            searching: Arc::new(AtomicBool::new(false)),
            cancel_flag: Arc::new(AtomicBool::new(false)),
            pondering: Arc::new(AtomicBool::new(false)),
            ponder_enabled: AtomicBool::new(false),
            ponder_start_time: None,
            ponder_time_params: None,
            expected_ponder_move: None,
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
                uci_println!("info string Polyglot book loaded: {}", path);
                let (entries, first_key, last_key) = book.debug_info();
                uci_println!("info string Book entries: {}", entries);
                if let Some(first_key) = first_key {
                    uci_println!("info string First entry key: 0x{:016X}", first_key);
                }
                if let Some(last_key) = last_key {
                    uci_println!("info string Last entry key: 0x{:016X}", last_key);
                }
                self.book = Some(book);
            }
            Err(e) => {
                uci_println!("info string Failed to load Polyglot book: {}", e);
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
                uci_println!("id name z-slon 0.5.0");
                uci_println!("id author hedgegod");
                uci_println!("option name Hash type spin default 16 min 1 max 33554432");
                uci_println!("option name Threads type spin default 1 min 1 max 512");
                uci_println!("option name UCI_Chess960 type check default false");
                uci_println!("option name Ponder type check default false");
                uci_println!("option name MultiPV type spin default 1 min 1 max 500");
                uci_println!("option name Move Overhead type spin default 10 min 0 max 5000");
                uci_println!("option name nodestime type spin default 0 min 0 max 10000");
                uci_println!("option name EvalFile type string default <embedded>");
                uci_println!("option name Book type string default <empty>");
                uci_println!("uciok");
            }
            "isready" => {
                uci_println!("readyok");
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
            "ponderhit" => {
                if self.pondering.load(Ordering::SeqCst) && self.searching.load(Ordering::SeqCst) {
                    self.pondering.store(false, Ordering::SeqCst);
                    uci_println!("info string Ponder hit - stopping search");
                    
                    let cancel_clone = Arc::clone(&self.cancel_flag);
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(10)).await;
                        cancel_clone.store(true, Ordering::SeqCst);
                    });
                    
                    self.ponder_time_params = None;
                    self.ponder_start_time = None;
                }
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
                std::process::exit(0);
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
        let mut last_move = None;
        
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

        self.update_position_history();

        if idx < parts.len() && parts[idx] == "moves" {
            idx += 1;
            while idx < parts.len() {
                let move_str = parts[idx];
                if let Some(mv) = self.parse_uci_move(move_str) {
                    apply_move(&mut self.board, mv);
                    self.update_position_history();
                    last_move = Some(mv);
                }
                idx += 1;
            }
        }
        
        self.expected_ponder_move = last_move;
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
        let mut is_ponder = false;
        for &part in parts.iter() {
            if part == "ponder" {
                is_ponder = true;
                break;
            }
        }
        if !is_ponder {
            if let Some(ref book) = self.book {
                if let Some(book_move) = book.get_best_move(&self.board) {
                    uci_println!("info string Book move found");
                    uci_println!("bestmove {}", move_to_uci(book_move));
                    return;
                }
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
                "ponder" => {
                    i += 1;
                }
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
        if movetime.is_none() && !is_ponder {
            let our_time = if self.board.is_white_to_move() { wtime } else { btime };
            let our_inc = if self.board.is_white_to_move() { _winc } else { _binc }.unwrap_or(0);
            if let Some(time_left) = our_time {
                let overhead = 30u64;
                let usable = time_left.saturating_sub(overhead);
                let target = usable / 25 + our_inc * 3 / 4;
                let cap = (usable / 2).max(50);
                movetime = Some(target.clamp(50, cap));
                depth = 100;
            }
        }
        
        if is_ponder {
            self.ponder_time_params = Some((wtime, btime, movetime));
            self.ponder_start_time = Some(Instant::now());
            depth = 100;
            movetime = None;
        }

        let is_repetition_draw = self.is_draw_by_repetition();
        if is_repetition_draw {
            uci_println!("info string Draw by repetition");
        }

        let fallback_move = {
            let moves = legal_moves(&self.board);
            if !moves.is_empty() {
                Some(moves[0])
            } else {
                None
            }
        };

        self.searching.store(true, Ordering::SeqCst);
        self.cancel_flag.store(false, Ordering::SeqCst);
        self.pondering.store(is_ponder, Ordering::SeqCst);

        let board_clone = self.board.clone();
        let threads = self.threads.load(Ordering::SeqCst);
        let cancel_clone = Arc::clone(&self.cancel_flag);
        let searching_clone = Arc::clone(&self.searching);

        let start_time = Instant::now();

        if let Some(mt) = movetime {
            let cancel_for_time = Arc::clone(&self.cancel_flag);
            let pondering_for_time = Arc::clone(&self.pondering);
            tokio::spawn(async move {
                tokio::time::sleep(Duration::from_millis(mt)).await;
                if !pondering_for_time.load(Ordering::SeqCst) {
                    cancel_for_time.store(true, Ordering::SeqCst);
                }
            });
        }

        let nnue_clone = if self.nnue.is_loaded() {
            Some(self.nnue.clone_handle())
        } else {
            None
        };
        
        if self.nnue.is_loaded() {
            if let Some(path) = self.nnue.path() {
                uci_println!("info string Using NNUE evaluation: {}", path);
            } else {
                uci_println!("info string Using NNUE evaluation");
            }
        } else {
            uci_println!("info string Using HCE evaluation");
        }
        
        let history = self.position_history.clone();
        let multi_pv = self.multi_pv.load(Ordering::Relaxed) as usize;
        
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = tokio::task::spawn_blocking(move || {
            search_position(board_clone, depth, threads, history, cancel_clone, nnue_clone, multi_pv, |event| {
                let _ = tx.send(event);
            })
        });

        let pondering_clone = Arc::clone(&self.pondering);
        let expected_ponder_move = self.expected_ponder_move;
        let is_ponder_search = is_ponder;
        tokio::spawn(async move {
            let mut best_move = fallback_move;
            let mut ponder_move = None;
            let mut best_depth = 0;
            let mut pv_index = 1;
            let mut last_depth = 0;

            while let Some(event) = rx.recv().await {
                if event.depth != last_depth {
                    pv_index = 1;
                    last_depth = event.depth;
                }
                
                if pv_index == 1 {
                    if let Some(mv) = event.best_move {
                        let is_valid = if is_ponder_search {
                            if let Some(expected_mv) = expected_ponder_move {
                                true
                            } else {
                                false
                            }
                        } else {
                            true
                        };
                        
                        if is_valid && event.depth >= best_depth {
                            best_move = Some(mv);
                            best_depth = event.depth;
                            if !is_ponder_search && event.pv_len > 1 && event.pv[0] == Some(mv) {
                                ponder_move = event.pv[1];
                            } else {
                                ponder_move = None;
                            }
                        }
                    }
                }
                
                let elapsed = start_time.elapsed().as_millis() as u64;
                
                uci_print!("info depth {} multipv {} score cp {} nodes {}", 
                    event.depth, pv_index, event.score, event.nodes);
                
                if elapsed > 0 {
                    let nps = (event.nodes as u64 * 1000) / elapsed;
                    uci_print!(" nps {} time {}", nps, elapsed);
                }
                
                if event.pv_len > 0 {
                    uci_print!(" pv");
                    for i in 0..event.pv_len {
                        if let Some(mv) = event.pv[i] {
                            uci_print!(" {}", move_to_uci(mv));
                        }
                    }
                }
                
                uci_println!();
                pv_index += 1;
            }

            let _ = handle.await;
            searching_clone.store(false, Ordering::SeqCst);
            pondering_clone.store(false, Ordering::SeqCst);
            if let Some(mv) = best_move {
                if let Some(pm) = ponder_move {
                    uci_println!("bestmove {} ponder {}", move_to_uci(mv), move_to_uci(pm));
                } else {
                    uci_println!("bestmove {}", move_to_uci(mv));
                }
            } else {
                uci_println!("bestmove 0000");
            }
        });
    }

    fn handle_setoption(&mut self, parts: &[&str]) {
        if parts.len() < 2 || parts[0] != "name" {
            return;
        }

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
                            uci_println!("info string Threads set to {}", t.clamp(1, 512));
                        }
                    }
                    "hash" => {
                        if let Ok(h) = value.parse::<u32>() {
                            self.hash_size.store(h.clamp(1, 33554432), Ordering::SeqCst);
                            uci_println!("info string Hash size set to {} MB", h.clamp(1, 33554432));
                        }
                    }
                    "book" => {
                        if !value.is_empty() && value != "<empty>" {
                            self.load_book(&value);
                        }
                    }
                    "evalfile" => {
                        if value == "<embedded>" {
                            uci_println!("info string Using embedded NNUE");
                        } else {
                            self.handle_evalfile_export(&value);
                        }
                    }
                    "multipv" => {
                        if !value.is_empty() {
                            if let Ok(pv_count) = value.parse::<u32>() {
                                self.multi_pv.store(pv_count.max(1).min(500), Ordering::Relaxed);
                            }
                        }
                    }
                    "ponder" => {
                        let enabled = value.to_lowercase() == "true";
                        self.ponder_enabled.store(enabled, Ordering::SeqCst);
                        uci_println!("info string Ponder {}", if enabled { "enabled" } else { "disabled" });
                    }
                    "uci_chess960" | "move overhead" | "nodestime" => {
                        uci_println!("info string Option {} not yet implemented", option_name);
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
                uci_println!("info string Exported embedded NNUE to: {}", path);
                self.nnue_file = Some(path.to_string());
            }
            Err(e) => {
                uci_println!("info string Failed to export NNUE: {}", e);
            }
        }
    }
    
    fn handle_use_nnue(&mut self, parts: &[&str]) {
        if parts.is_empty() {
            uci_println!("info string Error: missing NNUE path");
            return;
        }
        
        let path = parts.join(" ");
        match self.nnue.load(&path) {
            Ok(_) => {
                uci_println!("info string NNUE loaded: {}", path);
                uci_println!("info string Using NNUE evaluation");
            }
            Err(e) => {
                uci_println!("info string Failed to load NNUE: {}", e);
            }
        }
    }
}

impl UciEngine {
    #[allow(dead_code)]
    pub fn set_position_from_fen(&mut self, fen: &str) {
        self.board = Board::from_fen(fen);
        self.clear_position_history();
        self.update_position_history();
    }
    
    #[allow(dead_code)]
    pub fn get_best_move(&self, _depth: u32) -> Option<String> {
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
