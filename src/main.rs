mod board;
mod eval;
mod movegen;
mod search;
mod uci;
mod nnue;
mod polyglot_integration;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use board::{Board, Piece};
use eval::{evaluate, EvaluationBreakdown};
use movegen::{apply_move, is_in_check, legal_moves, Move};
use search::search_position;
use nnue::NnueEvaluator;
use tokio::sync::mpsc;
use rustyline::error::ReadlineError;
use rustyline::DefaultEditor;
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "z-slon")]
#[command(about = "A chess engine with NNUE support", long_about = None)]
struct Args {
    /// Enable CLI mode (default is UCI mode)
    #[arg(long)]
    cli: bool,
    
    /// Path to NNUE file
    #[arg(long)]
    nnue: Option<String>,
    
    /// Path to Polyglot book file
    #[arg(long)]
    book: Option<String>,
    
    /// Number of threads
    #[arg(long, default_value_t = 1)]
    threads: u32,
    
    /// Enable debug output
    #[arg(long)]
    debug: bool,
}

// Global debug flag
static DEBUG_MODE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn is_debug_mode() -> bool {
    DEBUG_MODE.load(Ordering::SeqCst)
}

pub fn set_debug_mode(enabled: bool) {
    DEBUG_MODE.store(enabled, Ordering::SeqCst);
}

struct EngineState {
    depth: AtomicU32,
    threads: AtomicU32,
    nnue: Arc<tokio::sync::RwLock<NnueEvaluator>>,
    position_history: Arc<tokio::sync::RwLock<Vec<u64>>>,
}

impl EngineState {
    fn new(depth: u32, threads: u32, nnue: NnueEvaluator) -> Self {
        Self {
            depth: AtomicU32::new(depth),
            threads: AtomicU32::new(threads),
            nnue: Arc::new(tokio::sync::RwLock::new(nnue)),
            position_history: Arc::new(tokio::sync::RwLock::new(vec![])),
        }
    }

    fn depth(&self) -> u32 {
        self.depth.load(Ordering::SeqCst)
    }

    fn set_depth(&self, depth: u32) {
        self.depth.store(depth, Ordering::SeqCst);
    }

    fn threads(&self) -> u32 {
        self.threads.load(Ordering::SeqCst)
    }

    fn set_threads(&self, threads: u32) {
        self.threads.store(threads, Ordering::SeqCst);
    }
}

#[tokio::main]
async fn main() {
    // Parse command line arguments
    let args = Args::parse();
    
    // Set debug mode
    set_debug_mode(args.debug);
    
    // Initialize NNUE evaluator
    let nnue_evaluator = NnueEvaluator::new();
    if let Some(nnue_path) = &args.nnue {
        match nnue_evaluator.load(nnue_path) {
            Ok(_) => {
                if is_debug_mode() {
                    eprintln!("NNUE loaded from: {}", nnue_path);
                }
            }
            Err(e) => eprintln!("Failed to load NNUE: {}", e),
        }
    }
    
    // Default mode is UCI, unless --cli flag is provided
    if !args.cli {
        let mut uci_engine = uci::UciEngine::new_with_nnue(nnue_evaluator);
        uci_engine.set_threads(args.threads);
        if let Some(book_path) = &args.book {
            uci_engine.load_book(book_path);
        }
        uci_engine.run().await;
        return;
    }

    let state = Arc::new(EngineState::new(6, args.threads, nnue_evaluator));
    let board = Arc::new(tokio::sync::RwLock::new(Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")));
    let eval_running = Arc::new(AtomicBool::new(false));
    let cancel_flag = Arc::new(AtomicBool::new(false));
    let position_changed = Arc::new(AtomicBool::new(false));
    
    eprintln!("Welcome to z-slon! Type commands (`set`, `show`, `move`, `eval`, `start`, `stop`, `exit`).");
    
    // Print NNUE status
    if is_debug_mode() {
        let nnue_guard = state.nnue.read().await;
        if nnue_guard.is_loaded() {
            if let Some(path) = nnue_guard.path() {
                eprintln!("NNUE evaluation enabled: {}", path);
            }
        } else {
            eprintln!("Using HCE (Hand-Crafted Evaluation)");
        }
    }
    
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<String>();
    
    // Spawn input reader task with rustyline
    let input_handle = tokio::task::spawn_blocking(move || {
        let mut rl = DefaultEditor::new().unwrap();
        loop {
            let readline = rl.readline(">> ");
            match readline {
                Ok(line) => {
                    let trimmed = line.trim();
                    if !trimmed.is_empty() {
                        if cmd_tx.send(trimmed.to_string()).is_err() {
                            break;
                        }
                    }
                }
                Err(ReadlineError::Interrupted) | Err(ReadlineError::Eof) => {
                    let _ = cmd_tx.send("exit".to_string());
                    break;
                }
                Err(_) => break,
            }
        }
    });
    
    // Main command processing loop
    while let Some(command) = cmd_rx.recv().await {
        if command.eq_ignore_ascii_case("exit") || command.eq_ignore_ascii_case("quit") {
            cancel_flag.store(true, Ordering::SeqCst);
            eval_running.store(false, Ordering::SeqCst);
            break;
        }
        
        match handle_command(&command, Arc::clone(&board), Arc::clone(&state), Arc::clone(&eval_running), Arc::clone(&cancel_flag), Arc::clone(&position_changed)).await {
            Ok(should_continue) => {
                if !should_continue {
                    cancel_flag.store(true, Ordering::SeqCst);
                    eval_running.store(false, Ordering::SeqCst);
                    break;
                }
            }
            Err(err) => {
                eprintln!("Error: {}", err);
            }
        }
    }
    
    input_handle.abort();
}

async fn handle_command(
    command: &str,
    board: Arc<tokio::sync::RwLock<Board>>,
    state: Arc<EngineState>,
    eval_running: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    position_changed: Arc<AtomicBool>,
) -> Result<bool, String> {
    let mut parts = command.split_whitespace();
    let cmd = parts
        .next()
        .ok_or_else(|| "Empty command".to_string())?
        .to_lowercase();
    match cmd.as_str() {
        "set" => handle_set_command(parts, Arc::clone(&board), Arc::clone(&state), Arc::clone(&position_changed), Arc::clone(&cancel_flag)).await,
        "show" => handle_show_command(parts, Arc::clone(&board)).await.map(|_| true),
        "eval" => handle_eval_command(Arc::clone(&board)).await,
        "start" => handle_start_command(parts, Arc::clone(&board), Arc::clone(&state), Arc::clone(&eval_running), Arc::clone(&cancel_flag), Arc::clone(&position_changed)).await,
        "stop" => handle_stop_command(Arc::clone(&eval_running), Arc::clone(&cancel_flag)).await,
        "move" => handle_move_command(parts, Arc::clone(&board), Arc::clone(&state), Arc::clone(&position_changed), Arc::clone(&cancel_flag)).await,
        _ => Err(format!("Unknown command: {}", cmd)),
    }
}

async fn handle_set_command<'a, I>(mut parts: I, board: Arc<tokio::sync::RwLock<Board>>, state: Arc<EngineState>, position_changed: Arc<AtomicBool>, cancel_flag: Arc<AtomicBool>) -> Result<bool, String>
where
    I: Iterator<Item = &'a str>,
{
    let target = parts
        .next()
        .ok_or_else(|| "Usage: set <board|depth> ...".to_string())?
        .to_lowercase();
    match target.as_str() {
        "board" => {
            let fen: String = parts.collect::<Vec<_>>().join(" ");
            if fen.is_empty() {
                return Err("Usage: set board <fen>".to_string());
            }
            let mut board_guard = board.write().await;
            *board_guard = Board::from_fen(&fen);
            drop(board_guard);
            {
                let mut hist = state.position_history.write().await;
                hist.clear();
                hist.push(board.read().await.position_hash());
            }
            cancel_flag.store(true, Ordering::SeqCst);
            position_changed.store(true, Ordering::SeqCst);
            eprintln!("Board updated.");
            Ok(true)
        }
        "depth" => {
            let value = parts
                .next()
                .ok_or_else(|| "Usage: set depth <1-100>".to_string())?;
            let depth: u32 = value.parse().map_err(|_| "Depth must be a number".to_string())?;
            if depth < 1 || depth > 100 {
                return Err("Depth range is 1..=100".to_string());
            }
            state.set_depth(depth);
            eprintln!("Depth set to {}.", depth);
            Ok(true)
        }
        "threads" => {
            let value = parts
                .next()
                .ok_or_else(|| "Usage: set threads <1-64>".to_string())?;
            let threads: u32 = value.parse().map_err(|_| "Threads must be a number".to_string())?;
            if threads < 1 || threads > 64 {
                return Err("Threads range is 1..=64".to_string());
            }
            state.set_threads(threads);
            eprintln!("Threads set to {}.", threads);
            Ok(true)
        }
        _ => Err(format!("Unknown set target: {}", target)),
    }
}

async fn handle_show_command<'a, I>(mut parts: I, board: Arc<tokio::sync::RwLock<Board>>) -> Result<(), String>
where
    I: Iterator<Item = &'a str>,
{
    let what = parts
        .next()
        .ok_or_else(|| "Usage: show <board|moves|full>".to_string())?
        .to_lowercase();
    let board_guard = board.read().await;
    match what.as_str() {
        "board" => {
            println!("{}", *board_guard);
        }
        "moves" => {
            let moves = legal_moves(&board_guard);
            print_moves_list(&moves);
        }
        "full" => {
            println!("{}", *board_guard);
            let moves = legal_moves(&board_guard);
            print_moves_list(&moves);
            print_full_info(&board_guard, &moves);
        }
        _ => return Err(format!("Unknown show target: {}", what)),
    }
    Ok(())
}

async fn handle_eval_command(board: Arc<tokio::sync::RwLock<Board>>) -> Result<bool, String> {
    let board_guard = board.read().await;
    let breakdown = evaluate(&board_guard);
    print_evaluation(&breakdown);
    Ok(true)
}

async fn handle_start_command<'a, I>(
    mut parts: I,
    board: Arc<tokio::sync::RwLock<Board>>,
    state: Arc<EngineState>,
    eval_running: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    position_changed: Arc<AtomicBool>,
) -> Result<bool, String>
where
    I: Iterator<Item = &'a str>,
{
    let subcmd = parts
        .next()
        .ok_or_else(|| "Usage: start eval".to_string())?
        .to_lowercase();
    match subcmd.as_str() {
        "eval" => {
            if eval_running.load(Ordering::SeqCst) {
                return Err("Eval already running".to_string());
            }
            eval_running.store(true, Ordering::SeqCst);
            cancel_flag.store(false, Ordering::SeqCst);
            position_changed.store(false, Ordering::SeqCst);
            
            tokio::spawn(run_continuous_search(
                Arc::clone(&board),
                Arc::clone(&state),
                Arc::clone(&eval_running),
                Arc::clone(&cancel_flag),
                Arc::clone(&position_changed),
            ));
            Ok(true)
        }
        _ => Err(format!("Unknown start target: {}", subcmd)),
    }
}

async fn handle_stop_command(eval_running: Arc<AtomicBool>, cancel_flag: Arc<AtomicBool>) -> Result<bool, String> {
    if !eval_running.load(Ordering::SeqCst) {
        return Err("Eval is not running".to_string());
    }
    eval_running.store(false, Ordering::SeqCst);
    cancel_flag.store(true, Ordering::SeqCst);
    eprintln!("Stopping eval...");
    Ok(true)
}

async fn handle_move_command<'a, I>(
    parts: I,
    board: Arc<tokio::sync::RwLock<Board>>,
    state: Arc<EngineState>,
    position_changed: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
) -> Result<bool, String>
where
    I: Iterator<Item = &'a str>,
{
    let notation = join_parts(parts);
    if notation.is_empty() {
        return Err("Usage: move <algebraic>".to_string());
    }
    
    let mut board_guard = board.write().await;
    let legal = legal_moves(&*board_guard);
    if legal.is_empty() {
        return Err("No legal moves available".to_string());
    }
    let mv = parse_move_notation(&notation, &*board_guard, &legal)?;
    let hash_before = board_guard.position_hash();
    apply_move(&mut *board_guard, mv);
    let hash_after = board_guard.position_hash();
    drop(board_guard);
    
    // Add the hashes to history
    {
        let mut hist = state.position_history.write().await;
        hist.push(hash_before);
        hist.push(hash_after);
    }
    
    // Cancel current search and signal position change to restart eval immediately
    cancel_flag.store(true, Ordering::SeqCst);
    position_changed.store(true, Ordering::SeqCst);
    eprintln!("Move {} applied.", mv);
    
    Ok(true)
}

fn join_parts<'a, I>(parts: I) -> String
where
    I: Iterator<Item = &'a str>,
{
    parts.collect::<Vec<_>>().join("")
}

fn parse_move_notation(notation: &str, board: &Board, legal: &[Move]) -> Result<Move, String> {
    let trimmed = notation.trim();
    if trimmed.is_empty() {
        return Err("Empty move".to_string());
    }

    let upper = trimmed.to_uppercase().replace('0', "O");
    if upper == "O-O" || upper == "O-O-O" {
        return parse_castling_move(&upper, board, legal);
    }

    let lower = trimmed.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    if chars.len() != 4 && chars.len() != 5 {
        return Err("Expected coordinate move like e2e4 or e7e8q".to_string());
    }

    let from = algebraic_to_square(chars[0], chars[1])
        .ok_or_else(|| "Invalid source square".to_string())?;
    let to = algebraic_to_square(chars[2], chars[3])
        .ok_or_else(|| "Invalid destination square".to_string())?;

    let promotion = if chars.len() == 5 {
        Some(promotion_piece(chars[4], board.is_white_to_move())?)
    } else {
        None
    };

    legal
        .iter()
        .copied()
        .find(|m| m.from == from && m.to == to && m.promotion == promotion)
        .ok_or_else(|| "Move is illegal".to_string())
}

fn parse_castling_move(notation: &str, board: &Board, legal: &[Move]) -> Result<Move, String> {
    let (from, to) = if board.is_white_to_move() {
        match notation {
            "O-O" => (4, 6),
            "O-O-O" => (4, 2),
            _ => unreachable!(),
        }
    } else {
        match notation {
            "O-O" => (60, 62),
            "O-O-O" => (60, 58),
            _ => unreachable!(),
        }
    };
    legal
        .iter()
        .copied()
        .find(|m| m.from == from && m.to == to)
        .ok_or_else(|| "Castling move is illegal".to_string())
}

fn algebraic_to_square(file: char, rank: char) -> Option<u8> {
    if !file.is_ascii_lowercase() || !('a'..='h').contains(&file) {
        return None;
    }
    if !rank.is_ascii_digit() || !('1'..='8').contains(&rank) {
        return None;
    }
    let file_idx = (file as u8) - b'a';
    let rank_idx = (rank as u8) - b'1';
    Some(rank_idx * 8 + file_idx)
}

fn promotion_piece(ch: char, white: bool) -> Result<Piece, String> {
    match ch {
        'q' | 'Q' => Ok(if white { Piece::WQueen } else { Piece::BQueen }),
        'r' | 'R' => Ok(if white { Piece::WRook } else { Piece::BRook }),
        'b' | 'B' => Ok(if white { Piece::WBishop } else { Piece::BBishop }),
        'n' | 'N' => Ok(if white { Piece::WKnight } else { Piece::BKnight }),
        _ => Err("Unsupported promotion piece".to_string()),
    }
}

fn print_moves_list(moves: &[Move]) {
    println!("Legal moves ({}):", moves.len());
    for m in moves {
        println!("{}", m);
    }
}

fn print_full_info(board: &Board, moves: &[Move]) {
    let side = if board.is_white_to_move() { "White" } else { "Black" };
    println!("Side to move: {}", side);
    if is_in_check(board, board.is_white_to_move()) {
        println!("Check: {} to move is in check", side);
    }
    if moves.is_empty() {
        if is_in_check(board, board.is_white_to_move()) {
            println!("Game status: checkmate");
        } else {
            println!("Game status: stalemate");
        }
    } else {
        println!("Game status: ongoing");
    }
    println!("En passant: {}", format_en_passant(board));
    println!("Castling: {}", format_castling(board.castling_rights()));
    println!("Halfmove clock: {}", board.halfmove_clock());
    println!("Fullmove number: {}", board.fullmove_number());
    println!("FEN: {}", board.to_fen());
}

fn format_en_passant(board: &Board) -> String {
    match board.en_passant_file() {
        Some(file) => format!("{}{}", (b'a' + file) as char, if board.is_white_to_move() { '6' } else { '3' }),
        None => "-".to_string(),
    }
}

fn format_castling(rights: u8) -> String {
    let mut s = String::new();
    if rights & 1 != 0 {
        s.push('K');
    }
    if rights & 2 != 0 {
        s.push('Q');
    }
    if rights & 4 != 0 {
        s.push('k');
    }
    if rights & 8 != 0 {
        s.push('q');
    }
    if s.is_empty() {
        s.push('-');
    }
    s
}

fn print_evaluation(eval: &EvaluationBreakdown) {
    let label = if eval.total > 0 {
        "white advantage"
    } else if eval.total < 0 {
        "black advantage"
    } else {
        "equal"
    };
    println!("Evaluation: {} ({})", format_signed(eval.total), label);
    println!("Breakdown:");
    println!("Material: {}", format_signed(eval.material));
    println!("PST: {}", format_signed(eval.pst));
    println!("Center: {}", format_signed(eval.center));
    println!("Mobility: {}", format_signed(eval.mobility));
    println!("King safety: {}", format_signed(eval.king_safety));
}

fn format_signed(value: i32) -> String {
    if value > 0 {
        format!("+{}", value)
    } else if value < 0 {
        format!("{}", value)
    } else {
        "0".to_string()
    }
}

async fn run_continuous_search(
    board: Arc<tokio::sync::RwLock<Board>>,
    state: Arc<EngineState>,
    eval_running: Arc<AtomicBool>,
    cancel_flag: Arc<AtomicBool>,
    position_changed: Arc<AtomicBool>,
) {
    loop {
        if !eval_running.load(Ordering::SeqCst) {
            break;
        }
        
        // Reset position changed flag and cancel flag
        position_changed.store(false, Ordering::SeqCst);
        cancel_flag.store(false, Ordering::SeqCst);
        
        let history = {
            let hist_guard = state.position_history.read().await;
            hist_guard.clone()
        };
        
        let board_clone = {
            let board_guard = board.read().await;
            board_guard.clone()
        };
        
        let depth = state.depth();
        let threads = state.threads();
        let cancel_clone = Arc::clone(&cancel_flag);
        let nnue_clone = {
            let nnue_guard = state.nnue.read().await;
            if nnue_guard.is_loaded() {
                Some(nnue_guard.clone_handle())
            } else {
                None
            }
        };
        
        let (tx, mut rx) = mpsc::unbounded_channel();
        let handle = tokio::task::spawn_blocking(move || {
            search_position(board_clone, depth, threads, history, cancel_clone, nnue_clone, 1, |event| {
                let _ = tx.send(event);
            })
        });

        // Print search progress
        let mut last_depth = 0;
        while let Some(event) = rx.recv().await {
            let mv_label = event
                .best_move
                .map(move_to_uci)
                .unwrap_or_else(|| "(none)".to_string());
            println!("depth {}: best move {}", event.depth, mv_label);
            last_depth = event.depth;
        }

        let _ = handle.await;
        
        // Check if we should stop
        if !eval_running.load(Ordering::SeqCst) {
            break;
        }
        
        // If cancelled by stop command, exit
        if cancel_flag.load(Ordering::SeqCst) && !position_changed.load(Ordering::SeqCst) {
            break;
        }
        
        // If we reached max depth without being cancelled, print full evaluation
        if last_depth == depth && !cancel_flag.load(Ordering::SeqCst) {
            let board_guard = board.read().await;
            let breakdown = evaluate(&*board_guard);
            drop(board_guard);
            print_evaluation(&breakdown);
        }
        
        // If we reached max depth, wait for position change
        while eval_running.load(Ordering::SeqCst) && !position_changed.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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