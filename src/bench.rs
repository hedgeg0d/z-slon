use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use crate::board::Board;
use crate::nnue::NnueEvaluator;
use crate::search::{search_position, Tt};

/// Fixed benchmark positions: opening-ish, sharp middlegames, tactical
/// shots and endgames. The benchmark is fully deterministic (single
/// thread, fresh TT per position, no timers), so `Nodes searched` must be
/// bit-identical between binaries with identical search logic. That makes
/// it a regression gate for performance-only patches: nodes stay equal,
/// nodes/sec shows the speedup.
pub const BENCH_FENS: [&str; 14] = [
    // start position
    "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
    // kiwipete: castling, ep, pins
    "r3k2r/p1ppqpb1/bn2pnp1/3PN3/1p2P3/2N2Q1p/PPPBBPPP/R3K2R w KQkq - 0 1",
    // endgame with rook activity
    "8/2p5/3p4/KP5r/1R3p1k/8/4P1P1/8 w - - 0 1",
    // promotions and heavy tactics
    "r3k2r/Pppp1ppp/1b3nbN/nP6/BBP1P3/q4N2/Pp1P2PP/R2Q1RK1 w kq - 0 1",
    // promotion to knight, checks
    "rnbq1k1r/pp1Pbppp/2p5/8/2B5/8/PPP1NnPP/RNBQK2R w KQ - 1 8",
    // quiet italian-ish middlegame
    "r4rk1/1pp1qppp/p1np1n2/2b1p1B1/2B1P1b1/P1NP1N2/1PP1QPPP/R4RK1 w - - 0 10",
    // sharp attacking position
    "2rr3k/pp3pp1/1nnqbN1p/3pN3/2pP4/2P3Q1/PPB4P/R4RK1 w - - 0 1",
    // maroczy bind middlegame
    "r1bq1rk1/pp2ppbp/2np2p1/2n5/2P1P3/2N2N2/PP2BPPP/R1BQ1RK1 w - - 0 9",
    // zugzwang study (B+P vs P)
    "8/8/4k3/8/2p5/8/B2P2K1/8 w - - 0 1",
    // queen + rook middlegame
    "3q1rk1/5pp1/5b1p/1p6/8/1P2Q1P1/P4PBP/3R2K1 w - - 0 1",
    // isolated queen pawn, IQP middlegame
    "r1b2rk1/pp1n1ppp/2p1pn2/q2p2B1/1bPP4/2N1P3/PPQN1PPP/R3KB1R w KQ - 0 1",
    // king + pawn endgame
    "6k1/5p2/6p1/8/7p/8/5PPP/6K1 w - - 0 1",
    // king + pawn endgame, race
    "8/5pk1/6p1/8/8/5PKP/8/8 w - - 0 1",
    // mutual zugzwang study
    "8/k7/3p4/p2P1p2/P2P1P2/8/8/K7 w - - 0 1",
];

const BENCH_TT_MB: usize = 16;

pub fn run(depth: u32, nnue: &NnueEvaluator) {
    let depth = depth.clamp(1, 100);
    println!("z-slon bench");
    println!("depth      : {}", depth);
    println!("threads    : 1 (deterministic)");
    println!("positions  : {}", BENCH_FENS.len());
    if nnue.is_loaded() {
        println!("eval       : nnue {}", nnue.path().unwrap_or_default());
    } else {
        println!("eval       : hce fallback");
    }
    println!();

    let tt = Arc::new(Tt::new(BENCH_TT_MB));
    let mut total_nodes = 0u64;
    let total_start = Instant::now();

    for (i, fen) in BENCH_FENS.iter().enumerate() {
        let board = Board::from_fen(fen);
        let history = vec![board.position_hash()];
        let cancel = Arc::new(AtomicBool::new(false));
        tt.clear();

        let mut nodes = 0u64;
        let mut reached = 0u32;
        let mut score = 0i32;
        let mut best_move = None;

        let start = Instant::now();
        search_position(
            board,
            depth,
            1,
            history,
            cancel,
            Some(nnue.clone_handle()),
            1,
            Arc::clone(&tt),
            |event| {
                nodes = event.nodes;
                reached = event.depth;
                score = event.score;
                best_move = event.best_move;
            },
        );
        let elapsed = start.elapsed();
        total_nodes += nodes;

        let ms = elapsed.as_millis() as u64;
        let nps = if ms > 0 { nodes * 1000 / ms } else { 0 };
        let mv = best_move
            .map(|m| m.to_string())
            .unwrap_or_else(|| "none".to_string());
        println!(
            "{:2}/{:<2} depth {:>2}  nodes {:>10}  time {:>6}ms  nps {:>7}  score {:>6}  best {:<6} {}",
            i + 1,
            BENCH_FENS.len(),
            reached,
            nodes,
            ms,
            nps,
            score,
            mv,
            fen
        );
    }

    let total_ms = total_start.elapsed().as_millis() as u64;
    let total_nps = if total_ms > 0 {
        total_nodes * 1000 / total_ms
    } else {
        0
    };
    println!();
    println!("===============================");
    println!("Total time (ms) : {}", total_ms);
    println!("Nodes searched  : {}", total_nodes);
    println!("Nodes/second    : {}", total_nps);
}
