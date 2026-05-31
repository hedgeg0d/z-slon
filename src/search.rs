use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::board::{Board, Piece};
use crate::eval::evaluate_with_nnue;
use crate::movegen::{apply_move, is_in_check, legal_captures, legal_moves, Move};
use crate::nnue::NnueEvaluator;
use nnue_rs::Accumulator;

const MATE_SCORE: i32 = 1_000_000;
const NEG_INF: i32 = -MATE_SCORE;
const POS_INF: i32 = MATE_SCORE;

const MAX_PLY: usize = 128;
const MAX_KILLERS: usize = 2;
const ACC_MAX_PLY: usize = 160;

const FUTILITY_MARGIN: i32 = 100;
const REVERSE_FUTILITY_MARGIN: i32 = 80;
const RAZORING_MARGIN: i32 = 300;
const DELTA_MARGIN: i32 = 200;
const ASPIRATION_WINDOW: i32 = 25;
const IID_DEPTH: u32 = 6;

const TT_BITS: usize = 22;
const TT_SIZE: usize = 1 << TT_BITS;
const TT_MASK: u64 = (TT_SIZE as u64) - 1;

#[derive(Clone, Copy, Debug)]
pub struct SearchEvent {
    pub depth: u32,
    pub seldepth: u32,
    pub best_move: Option<Move>,
    pub nodes: u64,
    pub score: i32,
    pub pv: [Option<Move>; 32],
    pub pv_len: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TTFlag {
    Exact,
    Lower,
    Upper,
}

#[derive(Clone, Copy, Debug)]
struct TTEntry {
    depth: u32,
    score: i32,
    flag: TTFlag,
    best_move: Option<Move>,
}

struct TtSlot {
    key: AtomicU64,
    data: AtomicU64,
}

pub struct Tt {
    slots: Vec<TtSlot>,
}

impl Tt {
    fn new() -> Self {
        let mut slots = Vec::with_capacity(TT_SIZE);
        for _ in 0..TT_SIZE {
            slots.push(TtSlot {
                key: AtomicU64::new(0),
                data: AtomicU64::new(0),
            });
        }
        Self { slots }
    }

    fn probe(&self, hash: u64) -> Option<TTEntry> {
        let idx = (hash & TT_MASK) as usize;
        let slot = &self.slots[idx];
        let data = slot.data.load(Ordering::Relaxed);
        let key = slot.key.load(Ordering::Relaxed);
        if data == 0 || (key ^ data) != hash {
            return None;
        }
        Some(decode_entry(data))
    }

    fn store(&self, hash: u64, depth: u32, score: i32, flag: TTFlag, best_move: Option<Move>) {
        let idx = (hash & TT_MASK) as usize;
        let slot = &self.slots[idx];
        let old_data = slot.data.load(Ordering::Relaxed);
        if old_data != 0 {
            let old_key = slot.key.load(Ordering::Relaxed);
            let same = (old_key ^ old_data) == hash;
            let old_depth = ((old_data >> 56) & 0xff) as u32;
            if same && old_depth > depth && flag != TTFlag::Exact {
                return;
            }
        }
        let data = encode_entry(depth, score, flag, best_move);
        slot.key.store(hash ^ data, Ordering::Relaxed);
        slot.data.store(data, Ordering::Relaxed);
    }
}

type TranspositionTable = Arc<Tt>;

fn encode_move(mv: Option<Move>) -> u16 {
    match mv {
        None => 0,
        Some(m) => {
            let promo = match m.promotion {
                None => 0u16,
                Some(Piece::WKnight) | Some(Piece::BKnight) => 1,
                Some(Piece::WBishop) | Some(Piece::BBishop) => 2,
                Some(Piece::WRook) | Some(Piece::BRook) => 3,
                Some(Piece::WQueen) | Some(Piece::BQueen) => 4,
                _ => 0,
            };
            (m.from as u16) | ((m.to as u16) << 6) | (promo << 12)
        }
    }
}

fn decode_move(v: u16) -> Option<Move> {
    if v == 0 {
        return None;
    }
    let from = (v & 0x3f) as u8;
    let to = ((v >> 6) & 0x3f) as u8;
    let promo = (v >> 12) & 0x7;
    let promotion = if promo == 0 {
        None
    } else {
        let white = to / 8 == 7;
        Some(match (promo, white) {
            (1, true) => Piece::WKnight,
            (1, false) => Piece::BKnight,
            (2, true) => Piece::WBishop,
            (2, false) => Piece::BBishop,
            (3, true) => Piece::WRook,
            (3, false) => Piece::BRook,
            (4, true) => Piece::WQueen,
            _ => Piece::BQueen,
        })
    };
    Some(Move { from, to, promotion })
}

fn encode_entry(depth: u32, score: i32, flag: TTFlag, best_move: Option<Move>) -> u64 {
    let m = encode_move(best_move) as u64;
    let s = (score as u32) as u64;
    let f = match flag {
        TTFlag::Exact => 0u64,
        TTFlag::Lower => 1,
        TTFlag::Upper => 2,
    };
    let d = (depth as u64) & 0xff;
    m | (s << 16) | (f << 48) | (d << 56)
}

fn decode_entry(data: u64) -> TTEntry {
    let best_move = decode_move((data & 0xffff) as u16);
    let score = ((data >> 16) & 0xffff_ffff) as u32 as i32;
    let flag = match (data >> 48) & 0x3 {
        0 => TTFlag::Exact,
        1 => TTFlag::Lower,
        _ => TTFlag::Upper,
    };
    let depth = ((data >> 56) & 0xff) as u32;
    TTEntry {
        depth,
        score,
        flag,
        best_move,
    }
}

#[derive(Clone)]
struct AccStack {
    stack: Vec<Accumulator>,
    enabled: bool,
}

impl AccStack {
    fn new(nnue: &Option<NnueEvaluator>) -> Self {
        if let Some(ev) = nnue {
            if let Some(a0) = ev.new_accumulator() {
                return Self {
                    stack: vec![a0; ACC_MAX_PLY],
                    enabled: true,
                };
            }
        }
        Self {
            stack: Vec::new(),
            enabled: false,
        }
    }

    fn root(&mut self, nnue: &Option<NnueEvaluator>, board: &Board) {
        if !self.enabled {
            return;
        }
        if let Some(ev) = nnue {
            ev.refresh(board, &mut self.stack[0]);
        }
    }

    fn descend(&mut self, nnue: &Option<NnueEvaluator>, ply: usize, parent: &Board, child: &Board) {
        if !self.enabled || ply + 1 >= self.stack.len() {
            return;
        }
        let (lo, hi) = self.stack.split_at_mut(ply + 1);
        if let Some(ev) = nnue {
            ev.update(parent, child, &lo[ply], &mut hi[0]);
        }
    }

    fn carry_null(&mut self, ply: usize) {
        if !self.enabled || ply + 1 >= self.stack.len() {
            return;
        }
        let (lo, hi) = self.stack.split_at_mut(ply + 1);
        hi[0].clone_from(&lo[ply]);
    }

    fn eval(&self, nnue: &Option<NnueEvaluator>, ply: usize, board: &Board) -> Option<i32> {
        if !self.enabled || ply >= self.stack.len() {
            return None;
        }
        let white_relative = nnue.as_ref().and_then(|ev| ev.eval_acc(&self.stack[ply], board))?;
        Some(if board.is_white_to_move() {
            white_relative
        } else {
            -white_relative
        })
    }
}

struct SearchTables {
    killer_moves: [[Option<Move>; MAX_KILLERS]; MAX_PLY],
    history: [[i32; 64]; 64],
    counter_moves: [[Option<Move>; 64]; 64],
    acc_stack: AccStack,
}

impl SearchTables {
    fn new(nnue: &Option<NnueEvaluator>) -> Self {
        Self {
            killer_moves: [[None; MAX_KILLERS]; MAX_PLY],
            history: [[0; 64]; 64],
            counter_moves: [[None; 64]; 64],
            acc_stack: AccStack::new(nnue),
        }
    }

    fn update_killer(&mut self, ply: usize, mv: Move) {
        if ply >= MAX_PLY {
            return;
        }
        if self.killer_moves[ply][0] == Some(mv) {
            return;
        }
        self.killer_moves[ply][1] = self.killer_moves[ply][0];
        self.killer_moves[ply][0] = Some(mv);
    }

    fn update_history(&mut self, mv: Move, depth: u32, bonus: bool) {
        let from = mv.from as usize;
        let to = mv.to as usize;
        let delta = (depth * depth) as i32;
        if bonus {
            self.history[from][to] += delta;
            if self.history[from][to] > 10000 {
                self.history[from][to] = 10000;
            }
        } else {
            self.history[from][to] -= delta;
            if self.history[from][to] < -10000 {
                self.history[from][to] = -10000;
            }
        }
    }

    fn get_history(&self, mv: Move) -> i32 {
        self.history[mv.from as usize][mv.to as usize]
    }

    fn is_killer(&self, ply: usize, mv: Move) -> bool {
        if ply >= MAX_PLY {
            return false;
        }
        self.killer_moves[ply][0] == Some(mv) || self.killer_moves[ply][1] == Some(mv)
    }

    fn set_counter_move(&mut self, prev_move: Move, counter: Move) {
        self.counter_moves[prev_move.from as usize][prev_move.to as usize] = Some(counter);
    }

    fn get_counter_move(&self, prev_move: Move) -> Option<Move> {
        self.counter_moves[prev_move.from as usize][prev_move.to as usize]
    }
}

pub fn search_position(
    board: Board,
    depth: u32,
    threads: u32,
    position_history: Vec<u64>,
    cancel_flag: Arc<AtomicBool>,
    nnue: Option<NnueEvaluator>,
    multi_pv: usize,
    mut on_progress: impl FnMut(SearchEvent),
) {
    let threads = threads.max(1).min(64);
    let nodes = Arc::new(AtomicU64::new(0));
    let tt: TranspositionTable = Arc::new(Tt::new());
    let mut position_history = position_history;
    if position_history.is_empty() || position_history.last() != Some(&board.position_hash()) {
        position_history.push(board.position_hash());
    }

    if depth == 0 {
        let event = SearchEvent {
            depth: 0,
            seldepth: 0,
            best_move: None,
            nodes: 0,
            score: 0,
            pv: [None; 32],
            pv_len: 0,
        };
        on_progress(event);
        return;
    }

    let best_result = Arc::new(Mutex::new((None, 0, [None; 32])));

    if threads == 1 {
        search_single_thread(
            board,
            depth,
            &nodes,
            &tt,
            &mut position_history,
            &cancel_flag,
            &nnue,
            multi_pv.max(1),
            &mut on_progress,
        );
    } else {
        search_lazy_smp(
            board,
            depth,
            threads,
            &nodes,
            &tt,
            &mut position_history,
            &cancel_flag,
            &nnue,
            multi_pv.max(1),
            &best_result,
            &mut on_progress,
        );
    }
}

fn search_single_thread(
    board: Board,
    depth: u32,
    nodes: &Arc<AtomicU64>,
    tt: &TranspositionTable,
    position_history: &mut PositionHistory,
    cancel_flag: &Arc<AtomicBool>,
    nnue: &Option<NnueEvaluator>,
    multi_pv: usize,
    on_progress: &mut impl FnMut(SearchEvent),
) {
    let mut prev_scores = vec![0; multi_pv];
    let mut tables = SearchTables::new(nnue);

    for current_depth in 1..=depth {
        if current_depth > 1 && cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let mut excluded_moves = Vec::new();

        for pv_idx in 0..multi_pv {
            let prev_score = prev_scores.get(pv_idx).copied().unwrap_or(0);

            let (depth_move, depth_score, pv) = search_root_aspiration(
                &board,
                current_depth,
                prev_score,
                nodes,
                tt,
                position_history,
                cancel_flag,
                nnue,
                &mut tables,
                &excluded_moves,
            );

            let was_cancelled = cancel_flag.load(Ordering::Relaxed);

            if !was_cancelled && depth_move.is_some() {
                if pv_idx < prev_scores.len() {
                    prev_scores[pv_idx] = depth_score;
                }

                let event = SearchEvent {
                    depth: current_depth,
                    seldepth: current_depth,
                    best_move: depth_move,
                    nodes: nodes.load(Ordering::Relaxed),
                    score: depth_score,
                    pv,
                    pv_len: pv.iter().take_while(|m| m.is_some()).count(),
                };
                on_progress(event);

                if let Some(mv) = depth_move {
                    excluded_moves.push(mv);
                }
            }

            if was_cancelled {
                break;
            }
        }

        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }
    }
}

fn search_lazy_smp(
    board: Board,
    depth: u32,
    threads: u32,
    nodes: &Arc<AtomicU64>,
    tt: &TranspositionTable,
    position_history: &mut PositionHistory,
    cancel_flag: &Arc<AtomicBool>,
    nnue: &Option<NnueEvaluator>,
    multi_pv: usize,
    best_result: &Arc<Mutex<(Option<Move>, i32, [Option<Move>; 32])>>,
    on_progress: &mut impl FnMut(SearchEvent),
) {
    use std::thread;

    let mut handles = vec![];

    for thread_id in 1..threads {
        let board = board.clone();
        let nodes = Arc::clone(nodes);
        let tt = Arc::clone(tt);
        let mut position_history = position_history.clone();
        let cancel_flag = Arc::clone(cancel_flag);
        let nnue = nnue.clone();
        let best_result = Arc::clone(best_result);

        let handle = thread::spawn(move || {
            helper_thread_search(
                board,
                depth,
                thread_id,
                multi_pv,
                &nodes,
                &tt,
                &mut position_history,
                &cancel_flag,
                &nnue,
                &best_result,
            );
        });

        handles.push(handle);
    }

    main_thread_search(
        board,
        depth,
        multi_pv,
        nodes,
        tt,
        position_history,
        cancel_flag,
        nnue,
        best_result,
        on_progress,
    );

    for handle in handles {
        let _ = handle.join();
    }
}

fn main_thread_search(
    board: Board,
    depth: u32,
    multi_pv: usize,
    nodes: &Arc<AtomicU64>,
    tt: &TranspositionTable,
    position_history: &mut PositionHistory,
    cancel_flag: &Arc<AtomicBool>,
    nnue: &Option<NnueEvaluator>,
    best_result: &Arc<Mutex<(Option<Move>, i32, [Option<Move>; 32])>>,
    on_progress: &mut impl FnMut(SearchEvent),
) {
    let mut prev_scores = vec![0; multi_pv];
    let mut tables = SearchTables::new(nnue);

    for current_depth in 1..=depth {
        if current_depth > 1 && cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let mut excluded_moves = Vec::new();

        for pv_idx in 0..multi_pv {
            let prev_score = prev_scores.get(pv_idx).copied().unwrap_or(0);

            let (depth_move, depth_score, pv) = search_root_aspiration(
                &board,
                current_depth,
                prev_score,
                nodes,
                tt,
                position_history,
                cancel_flag,
                nnue,
                &mut tables,
                &excluded_moves,
            );

            let was_cancelled = cancel_flag.load(Ordering::Relaxed);

            if !was_cancelled && depth_move.is_some() {
                if pv_idx < prev_scores.len() {
                    prev_scores[pv_idx] = depth_score;
                }

                if pv_idx == 0 {
                    if let Ok(mut best) = best_result.try_lock() {
                        if best.0.is_none() || depth_score > best.1 {
                            *best = (depth_move, depth_score, pv);
                        }
                    }
                }

                let event = SearchEvent {
                    depth: current_depth,
                    seldepth: current_depth,
                    best_move: depth_move,
                    nodes: nodes.load(Ordering::Relaxed),
                    score: depth_score,
                    pv,
                    pv_len: pv.iter().take_while(|m| m.is_some()).count(),
                };
                on_progress(event);

                if let Some(mv) = depth_move {
                    excluded_moves.push(mv);
                }
            }

            if was_cancelled {
                break;
            }
        }

        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }
    }
}

fn helper_thread_search(
    board: Board,
    depth: u32,
    thread_id: u32,
    multi_pv: usize,
    nodes: &Arc<AtomicU64>,
    tt: &TranspositionTable,
    position_history: &mut PositionHistory,
    cancel_flag: &Arc<AtomicBool>,
    nnue: &Option<NnueEvaluator>,
    best_result: &Arc<Mutex<(Option<Move>, i32, [Option<Move>; 32])>>,
) {
    let mut prev_scores = vec![0; multi_pv];
    let mut tables = SearchTables::new(nnue);

    let depth_offset = (thread_id % 3) as i32 - 1;

    for current_depth in 1..=depth {
        if current_depth > 1 && cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let search_depth = if current_depth > 3 {
            ((current_depth as i32 + depth_offset).max(1) as u32).min(depth)
        } else {
            current_depth
        };

        let mut excluded_moves = Vec::new();

        for pv_idx in 0..multi_pv {
            let prev_score = prev_scores.get(pv_idx).copied().unwrap_or(0);

            let (depth_move, depth_score, _pv) = search_root_aspiration(
                &board,
                search_depth,
                prev_score,
                nodes,
                tt,
                position_history,
                cancel_flag,
                nnue,
                &mut tables,
                &excluded_moves,
            );

            let was_cancelled = cancel_flag.load(Ordering::Relaxed);

            if !was_cancelled && depth_move.is_some() {
                if pv_idx < prev_scores.len() {
                    prev_scores[pv_idx] = depth_score;
                }

                if pv_idx == 0 {
                    if let Ok(mut best) = best_result.try_lock() {
                        if best.0.is_none() || depth_score > best.1 {
                            *best = (depth_move, depth_score, _pv);
                        }
                    }
                }

                if let Some(mv) = depth_move {
                    excluded_moves.push(mv);
                }
            }

            if was_cancelled {
                break;
            }
        }

        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }
    }
}

type PositionHistory = Vec<u64>;

fn search_root_aspiration(
    board: &Board,
    depth: u32,
    prev_score: i32,
    nodes: &Arc<AtomicU64>,
    tt: &TranspositionTable,
    position_history: &mut PositionHistory,
    cancel_flag: &Arc<AtomicBool>,
    nnue: &Option<NnueEvaluator>,
    tables: &mut SearchTables,
    excluded_moves: &[Move],
) -> (Option<Move>, i32, [Option<Move>; 32]) {
    let mut alpha = prev_score - ASPIRATION_WINDOW;
    let mut beta = prev_score + ASPIRATION_WINDOW;
    let mut delta = ASPIRATION_WINDOW;

    if depth <= 5 || prev_score.abs() > 500 {
        alpha = NEG_INF;
        beta = POS_INF;
    }

    loop {
        let (mv, score, pv) = search_root(board, depth, alpha, beta, nodes, tt, position_history, cancel_flag, nnue, tables, excluded_moves);

        if cancel_flag.load(Ordering::Relaxed) {
            return (mv, score, pv);
        }

        if score <= alpha {
            alpha -= delta;
            delta *= 2;
        } else if score >= beta {
            beta += delta;
            delta *= 2;
        } else {
            return (mv, score, pv);
        }

        if delta > 400 {
            alpha = NEG_INF;
            beta = POS_INF;
        }
    }
}

fn search_root(
    board: &Board,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    nodes: &Arc<AtomicU64>,
    tt: &TranspositionTable,
    position_history: &mut PositionHistory,
    cancel_flag: &Arc<AtomicBool>,
    nnue: &Option<NnueEvaluator>,
    tables: &mut SearchTables,
    excluded_moves: &[Move],
) -> (Option<Move>, i32, [Option<Move>; 32]) {
    let mut moves = legal_moves(board);
    if moves.is_empty() {
        return (None, terminal_score(board, 0), [None; 32]);
    }

    if !excluded_moves.is_empty() {
        moves.retain(|m| !excluded_moves.contains(m));
        if moves.is_empty() {
            return (None, 0, [None; 32]);
        }
    }

    let hash = board.position_hash();
    let tt_move = probe_tt(tt, hash).and_then(|e| e.best_move);
    order_moves(board, &mut moves, tt_move, None, 0, tables);

    tables.acc_stack.root(nnue, board);

    let mut best_move = None;
    let mut best_score = NEG_INF;
    let mut best_pv = [None; 32];
    let mut pv_line = [None; 32];

    for (i, &mv) in moves.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let mut next = board.clone();
        apply_move(&mut next, mv);

        let next_hash = next.position_hash();
        let repetition_count = count_repetitions(position_history, next_hash);

        if repetition_count >= 2 {
            continue;
        }

        if repetition_count >= 1 {
            let opponent_moves = legal_moves(&next);
            let mut can_force = false;
            for &opp_mv in &opponent_moves {
                let mut opp_board = next.clone();
                apply_move(&mut opp_board, opp_mv);
                let opp_hash = opp_board.position_hash();
                if count_repetitions(position_history, opp_hash) >= 2 {
                    can_force = true;
                    break;
                }
            }
            if can_force {
                continue;
            }
        }

        let gives_check = is_in_check(&next, next.is_white_to_move());
        let extension = if gives_check { 1 } else { 0 };
        let child_depth = depth - 1 + extension;

        position_history.push(next_hash);
        tables.acc_stack.descend(nnue, 0, board, &next);

        let score = if i == 0 {
            -pvs(&next, child_depth, -beta, -alpha, nodes, 1, tt, cancel_flag, nnue, &mut pv_line, position_history, tables, None)
        } else {
            let mut null_score = -pvs(&next, child_depth, -alpha - 1, -alpha, nodes, 1, tt, cancel_flag, nnue, &mut pv_line, position_history, tables, None);
            if null_score > alpha && null_score < beta {
                null_score = -pvs(&next, child_depth, -beta, -alpha, nodes, 1, tt, cancel_flag, nnue, &mut pv_line, position_history, tables, None);
            }
            null_score
        };

        position_history.pop();

        if score > best_score {
            best_score = score;
            best_move = Some(mv);
            best_pv[0] = Some(mv);
            for j in 0..31 {
                best_pv[j + 1] = pv_line[j];
            }
        }

        if score > alpha {
            alpha = score;
            if alpha >= beta {
                let is_quiet = !is_capture(board, mv);
                if is_quiet {
                    tables.update_history(mv, depth, true);
                }
                break;
            }
        }
    }

    if let Some(mv) = best_move {
        store_tt(tt, board.position_hash(), depth, best_score, TTFlag::Exact, Some(mv));
    }

    (best_move, best_score, best_pv)
}

fn pvs(
    board: &Board,
    depth: u32,
    mut alpha: i32,
    beta: i32,
    nodes: &AtomicU64,
    ply: u32,
    tt: &TranspositionTable,
    cancel_flag: &Arc<AtomicBool>,
    nnue: &Option<NnueEvaluator>,
    pv_line: &mut [Option<Move>; 32],
    position_history: &mut PositionHistory,
    tables: &mut SearchTables,
    prev_move: Option<Move>,
) -> i32 {
    if cancel_flag.load(Ordering::Relaxed) {
        return 0;
    }

    let is_pv = beta - alpha > 1;
    pv_line[0] = None;

    if board.is_draw_by_fifty_move() || board.is_insufficient_material() {
        return 0;
    }

    nodes.fetch_add(1, Ordering::Relaxed);

    let hash = board.position_hash();

    if let Some(entry) = probe_tt_cutoff(tt, hash, depth, alpha, beta) {
        return entry.score;
    }

    if count_repetitions(position_history, hash) >= 2 {
        return 0;
    }

    if depth == 0 {
        return quiescence(board, alpha, beta, nodes, ply, cancel_flag, nnue, position_history, tables);
    }

    let in_check = is_in_check(board, board.is_white_to_move());
    let static_eval = if in_check {
        NEG_INF
    } else {
        tables
            .acc_stack
            .eval(nnue, ply as usize, board)
            .unwrap_or_else(|| static_eval_with_nnue(board, nnue))
    };

    if !is_pv && !in_check && depth <= 3 {
        let razor_margin = RAZORING_MARGIN * depth as i32;
        if static_eval + razor_margin < alpha {
            let q_score = quiescence(board, alpha - razor_margin, alpha - razor_margin + 1, nodes, ply, cancel_flag, nnue, position_history, tables);
            if q_score + razor_margin <= alpha {
                return q_score;
            }
        }
    }

    if !is_pv && !in_check && depth <= 7 && has_non_pawn_material(board) {
        let rfp_margin = REVERSE_FUTILITY_MARGIN * depth as i32;
        if static_eval - rfp_margin >= beta {
            return static_eval - rfp_margin;
        }
    }

    if !is_pv && !in_check && depth >= 3 && has_non_pawn_material(board) && static_eval >= beta {
        let mut null_board = board.clone();
        null_board.white_to_move = !null_board.white_to_move;
        let r = if depth >= 6 { 3 } else { 2 };
        let null_depth = depth.saturating_sub(1 + r);
        tables.acc_stack.carry_null(ply as usize);
        let null_score = -pvs(&null_board, null_depth, -beta, -beta + 1, nodes, ply + 1, tt, cancel_flag, nnue, &mut [None; 32], position_history, tables, None);
        if null_score >= beta {
            if depth >= 12 {
                let verify_depth = depth.saturating_sub(r + 3);
                let verify_score = pvs(board, verify_depth, beta - 1, beta, nodes, ply, tt, cancel_flag, nnue, &mut [None; 32], position_history, tables, prev_move);
                if verify_score >= beta {
                    return beta;
                }
            } else {
                return beta;
            }
        }
    }

    let mut tt_move = probe_tt(tt, hash).and_then(|e| e.best_move);
    if is_pv && tt_move.is_none() && depth >= IID_DEPTH {
        let iid_depth = depth.saturating_sub(2);
        pvs(board, iid_depth, alpha, beta, nodes, ply, tt, cancel_flag, nnue, &mut [None; 32], position_history, tables, prev_move);
        tt_move = probe_tt(tt, hash).and_then(|e| e.best_move);
    }

    let mut moves = legal_moves(board);
    if moves.is_empty() {
        return terminal_score(board, ply);
    }

    order_moves(board, &mut moves, tt_move, prev_move, ply as usize, tables);

    let mut best_score = NEG_INF;
    let mut best_move = None;
    let mut flag = TTFlag::Upper;
    let mut child_pv = [None; 32];
    let mut quiets_tried = Vec::new();

    for (i, &mv) in moves.iter().enumerate() {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        let mut next = board.clone();
        apply_move(&mut next, mv);

        position_history.push(next.position_hash());

        let is_quiet = !is_capture(board, mv);
        let gives_check = is_in_check(&next, next.is_white_to_move());

        if !is_pv && !in_check && !gives_check && is_quiet && depth <= 7 && i > 0 {
            let futility_margin = FUTILITY_MARGIN * depth as i32;
            if static_eval + futility_margin <= alpha {
                position_history.pop();
                continue;
            }
        }

        if !is_pv && !in_check && is_quiet && depth <= 8 && i > 0 {
            if see(board, mv) < -(30 * depth as i32) {
                position_history.pop();
                continue;
            }
        }

        if !is_pv && !in_check && !gives_check && is_quiet && depth <= 6
            && i as u32 >= 3 + depth * depth
            && best_score > -MATE_SCORE + 1000
        {
            position_history.pop();
            continue;
        }

        let extension = if gives_check { 1 } else { 0 };
        let child_depth = depth - 1 + extension;

        let mut reduction = 0;

        if i >= 3 && depth >= 3 && is_quiet && !in_check && !gives_check {
            reduction = ((depth as f32).ln() * (i as f32).ln() / 2.0) as u32;

            let history_score = tables.get_history(mv);
            if history_score < 0 {
                reduction += 1;
            } else if history_score > 5000 {
                reduction = reduction.saturating_sub(1);
            }

            reduction = reduction.min(child_depth.saturating_sub(1));
        }

        tables.acc_stack.descend(nnue, ply as usize, board, &next);

        let score = if i == 0 {
            -pvs(&next, child_depth, -beta, -alpha, nodes, ply + 1, tt, cancel_flag, nnue, &mut child_pv, position_history, tables, Some(mv))
        } else {
            let reduced_depth = child_depth.saturating_sub(reduction);
            let mut null_score = -pvs(&next, reduced_depth, -alpha - 1, -alpha, nodes, ply + 1, tt, cancel_flag, nnue, &mut child_pv, position_history, tables, Some(mv));

            if null_score > alpha && reduction > 0 {
                null_score = -pvs(&next, child_depth, -alpha - 1, -alpha, nodes, ply + 1, tt, cancel_flag, nnue, &mut child_pv, position_history, tables, Some(mv));
            }

            if null_score > alpha && null_score < beta {
                null_score = -pvs(&next, child_depth, -beta, -alpha, nodes, ply + 1, tt, cancel_flag, nnue, &mut child_pv, position_history, tables, Some(mv));
            }
            null_score
        };

        position_history.pop();

        if is_quiet {
            quiets_tried.push(mv);
        }

        if score > best_score {
            best_score = score;
            best_move = Some(mv);

            if score > alpha {
                alpha = score;
                flag = TTFlag::Exact;

                pv_line[0] = Some(mv);
                for j in 0..31 {
                    pv_line[j + 1] = child_pv[j];
                }

                if alpha >= beta {
                    flag = TTFlag::Lower;

                    if is_quiet {
                        tables.update_killer(ply as usize, mv);
                        tables.update_history(mv, depth, true);

                        if let Some(pm) = prev_move {
                            tables.set_counter_move(pm, mv);
                        }

                        for &tried in &quiets_tried {
                            if tried != mv {
                                tables.update_history(tried, depth, false);
                            }
                        }
                    }
                    break;
                }
            }
        }
    }

    store_tt(tt, hash, depth, best_score, flag, best_move);
    best_score
}

fn quiescence(
    board: &Board,
    mut alpha: i32,
    beta: i32,
    nodes: &AtomicU64,
    ply: u32,
    cancel_flag: &Arc<AtomicBool>,
    nnue: &Option<NnueEvaluator>,
    position_history: &mut PositionHistory,
    tables: &mut SearchTables,
) -> i32 {
    if cancel_flag.load(Ordering::Relaxed) {
        return 0;
    }

    nodes.fetch_add(1, Ordering::Relaxed);

    let hash = board.position_hash();
    if count_repetitions(position_history, hash) >= 2 {
        return 0;
    }

    let in_check = is_in_check(board, board.is_white_to_move());
    let stand_pat = if in_check {
        NEG_INF
    } else {
        tables
            .acc_stack
            .eval(nnue, ply as usize, board)
            .unwrap_or_else(|| static_eval_with_nnue(board, nnue))
    };

    if !in_check && stand_pat >= beta {
        return beta;
    }

    if !in_check && stand_pat > alpha {
        alpha = stand_pat;
    }

    let mut moves = if in_check {
        legal_moves(board)
    } else {
        legal_captures(board)
    };

    if moves.is_empty() {
        if in_check {
            return terminal_score(board, ply);
        }
        return stand_pat;
    }

    moves.sort_by_cached_key(|&mv| {
        let victim = board.square(mv.to);
        let attacker = board.square(mv.from);
        let victim_val = piece_value_simple(victim);
        let attacker_val = piece_value_simple(attacker);
        -(victim_val * 10 - attacker_val)
    });

    for &mv in &moves {
        if cancel_flag.load(Ordering::Relaxed) {
            break;
        }

        if !in_check {
            let victim = board.square(mv.to);
            let capture_value = piece_value_simple(victim) * 100;
            if stand_pat + capture_value + DELTA_MARGIN < alpha {
                continue;
            }

            if see(board, mv) < 0 {
                continue;
            }
        }

        let mut next = board.clone();
        apply_move(&mut next, mv);

        position_history.push(next.position_hash());
        tables.acc_stack.descend(nnue, ply as usize, board, &next);

        let score = -quiescence(&next, -beta, -alpha, nodes, ply + 1, cancel_flag, nnue, position_history, tables);

        position_history.pop();

        if score >= beta {
            return beta;
        }

        if score > alpha {
            alpha = score;
        }
    }

    alpha
}

fn order_moves(
    board: &Board,
    moves: &mut Vec<Move>,
    tt_move: Option<Move>,
    prev_move: Option<Move>,
    ply: usize,
    tables: &SearchTables,
) {
    moves.sort_by_cached_key(|&mv| {
        if Some(mv) == tt_move {
            return -10_000_000;
        }

        let is_capture_move = is_capture(board, mv);

        if is_capture_move {
            let victim = board.square(mv.to);
            let attacker = board.square(mv.from);
            let victim_val = piece_value_simple(victim);
            let attacker_val = piece_value_simple(attacker);
            let mvv_lva = victim_val * 10 - attacker_val;

            let see_score = see(board, mv);
            if see_score >= 0 {
                return -(1_000_000 + mvv_lva * 100 + see_score);
            } else {
                return -(100_000 + mvv_lva * 100);
            }
        }

        if tables.is_killer(ply, mv) {
            return -500_000;
        }

        if let Some(pm) = prev_move {
            if Some(mv) == tables.get_counter_move(pm) {
                return -400_000;
            }
        }

        let history = tables.get_history(mv);
        -(history)
    });
}

fn see(board: &Board, mv: Move) -> i32 {
    let from = mv.from;
    let to = mv.to;

    let mut gain = vec![0i32];
    let mut attacker = board.square(from);
    let victim = board.square(to);

    gain.push(piece_value_simple(victim));

    if matches!(attacker, Piece::WPawn | Piece::BPawn) {
        if board.en_passant_file().is_some() && victim == Piece::Empty {
            let diff = (from as i8 - to as i8).abs();
            if diff == 7 || diff == 9 {
                gain[1] += piece_value_simple(Piece::WPawn);
            }
        }
    }

    let mut attacking_side = !board.is_white_to_move();
    let mut occupied = 0u64;
    for i in 0..64 {
        if board.square(i) != Piece::Empty {
            occupied |= 1u64 << i;
        }
    }
    occupied ^= 1u64 << from;

    let mut d = 1;

    while d < 16 {
        let next_attacker = find_least_valuable_attacker(board, to, attacking_side, occupied);
        if next_attacker.is_none() {
            break;
        }

        let (attacker_sq, attacker_piece) = next_attacker.unwrap();
        occupied ^= 1u64 << attacker_sq;

        gain.push(piece_value_simple(attacker) - gain[d]);
        attacker = attacker_piece;
        attacking_side = !attacking_side;
        d += 1;

        if matches!(attacker, Piece::WKing | Piece::BKing) {
            break;
        }
    }

    for i in (1..gain.len()).rev() {
        if i % 2 == 1 {
            gain[i - 1] = gain[i - 1].max(-gain[i]);
        } else {
            gain[i - 1] = gain[i - 1].min(-gain[i]);
        }
    }

    gain[0]
}

fn find_least_valuable_attacker(
    board: &Board,
    target: u8,
    side: bool,
    occupied: u64,
) -> Option<(u8, Piece)> {
    let piece_order = if side {
        vec![Piece::WPawn, Piece::WKnight, Piece::WBishop, Piece::WRook, Piece::WQueen, Piece::WKing]
    } else {
        vec![Piece::BPawn, Piece::BKnight, Piece::BBishop, Piece::BRook, Piece::BQueen, Piece::BKing]
    };

    for piece_type in piece_order {
        for sq in 0..64 {
            if (occupied & (1u64 << sq)) == 0 {
                continue;
            }

            let piece = board.square(sq);
            if piece != piece_type {
                continue;
            }

            if can_piece_attack(board, sq, target, piece, occupied) {
                return Some((sq, piece));
            }
        }
    }

    None
}

fn can_piece_attack(_board: &Board, from: u8, to: u8, piece: Piece, occupied: u64) -> bool {
    let from_rank = from / 8;
    let from_file = from % 8;
    let to_rank = to / 8;
    let to_file = to % 8;

    let rank_diff = (to_rank as i8 - from_rank as i8).abs();
    let file_diff = (to_file as i8 - from_file as i8).abs();

    match piece {
        Piece::WPawn => to_rank as i8 - from_rank as i8 == 1 && file_diff == 1,
        Piece::BPawn => from_rank as i8 - to_rank as i8 == 1 && file_diff == 1,
        Piece::WKnight | Piece::BKnight => {
            (rank_diff == 2 && file_diff == 1) || (rank_diff == 1 && file_diff == 2)
        }
        Piece::WBishop | Piece::BBishop => rank_diff == file_diff && is_path_clear(from, to, occupied),
        Piece::WRook | Piece::BRook => {
            (from_rank == to_rank || from_file == to_file) && is_path_clear(from, to, occupied)
        }
        Piece::WQueen | Piece::BQueen => {
            (rank_diff == file_diff || from_rank == to_rank || from_file == to_file) && is_path_clear(from, to, occupied)
        }
        Piece::WKing | Piece::BKing => rank_diff <= 1 && file_diff <= 1,
        Piece::Empty => false,
    }
}

fn is_path_clear(from: u8, to: u8, occupied: u64) -> bool {
    let from_rank = from / 8;
    let from_file = from % 8;
    let to_rank = to / 8;
    let to_file = to % 8;

    let rank_step = (to_rank as i8 - from_rank as i8).signum();
    let file_step = (to_file as i8 - from_file as i8).signum();

    let mut current_rank = from_rank as i8 + rank_step;
    let mut current_file = from_file as i8 + file_step;

    while current_rank != to_rank as i8 || current_file != to_file as i8 {
        let sq = (current_rank as u8) * 8 + (current_file as u8);
        if (occupied & (1u64 << sq)) != 0 {
            return false;
        }
        current_rank += rank_step;
        current_file += file_step;
    }

    true
}

fn piece_value_simple(piece: Piece) -> i32 {
    match piece {
        Piece::WPawn | Piece::BPawn => 1,
        Piece::WKnight | Piece::BKnight => 3,
        Piece::WBishop | Piece::BBishop => 3,
        Piece::WRook | Piece::BRook => 5,
        Piece::WQueen | Piece::BQueen => 9,
        Piece::WKing | Piece::BKing => 0,
        Piece::Empty => 0,
    }
}

fn static_eval_with_nnue(board: &Board, nnue: &Option<NnueEvaluator>) -> i32 {
    let eval = evaluate_with_nnue(board, nnue);
    if board.is_white_to_move() {
        eval
    } else {
        -eval
    }
}

fn terminal_score(board: &Board, ply: u32) -> i32 {
    if is_in_check(board, board.is_white_to_move()) {
        -(MATE_SCORE - ply as i32)
    } else {
        0
    }
}

fn is_capture(board: &Board, mv: Move) -> bool {
    let piece = board.square(mv.from);
    let target = board.square(mv.to);
    if target != Piece::Empty {
        return true;
    }
    if matches!(piece, Piece::WPawn | Piece::BPawn) {
        let diff = (mv.from as i8 - mv.to as i8).abs();
        if diff == 7 || diff == 9 {
            return board.en_passant.is_some();
        }
    }
    false
}

fn has_non_pawn_material(board: &Board) -> bool {
    for sq in 0..64 {
        let piece = board.square(sq);
        match piece {
            Piece::WKnight | Piece::WBishop | Piece::WRook | Piece::WQueen |
            Piece::BKnight | Piece::BBishop | Piece::BRook | Piece::BQueen => return true,
            _ => {}
        }
    }
    false
}

fn probe_tt(tt: &TranspositionTable, hash: u64) -> Option<TTEntry> {
    tt.probe(hash)
}

fn probe_tt_cutoff(tt: &TranspositionTable, hash: u64, depth: u32, alpha: i32, beta: i32) -> Option<TTEntry> {
    let entry = tt.probe(hash)?;

    if entry.depth >= depth {
        match entry.flag {
            TTFlag::Exact => return Some(entry),
            TTFlag::Lower if entry.score >= beta => return Some(entry),
            TTFlag::Upper if entry.score <= alpha => return Some(entry),
            _ => {}
        }
    }

    None
}

fn store_tt(tt: &TranspositionTable, hash: u64, depth: u32, score: i32, flag: TTFlag, best_move: Option<Move>) {
    tt.store(hash, depth, score, flag, best_move);
}

fn count_repetitions(hist: &Vec<u64>, hash: u64) -> usize {
    hist.iter().filter(|&&h| h == hash).count()
}
