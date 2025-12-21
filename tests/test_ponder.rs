use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::time::{Duration, Instant};

use z_slon::board::Board;
use z_slon::movegen::{apply_move, legal_moves, Move};

fn spawn_engine() -> (Child, ChildStdin, BufReader<std::process::ChildStdout>) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_z-slon"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("Failed to start engine");

    let stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    (child, stdin, BufReader::new(stdout))
}

fn send(stdin: &mut ChildStdin, cmd: &str) {
    writeln!(stdin, "{cmd}").unwrap();
    stdin.flush().unwrap();
}

fn read_bestmove(reader: &mut BufReader<std::process::ChildStdout>, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    let mut line = String::new();

    while Instant::now() < deadline {
        line.clear();
        let n = reader.read_line(&mut line).unwrap();
        if n == 0 {
            break;
        }
        let s = line.trim().to_string();
        if s.starts_with("bestmove ") {
            return s;
        }
    }

    panic!("Timed out waiting for bestmove");
}

fn parse_bestmove(line: &str) -> (String, Option<String>) {
    let parts: Vec<&str> = line.split_whitespace().collect();
    assert!(
        parts.len() >= 2 && parts[0] == "bestmove",
        "bad bestmove line: {line}"
    );
    let bm = parts[1].to_string();
    let ponder = if parts.len() >= 4 && parts[2] == "ponder" {
        Some(parts[3].to_string())
    } else {
        None
    };
    (bm, ponder)
}

fn parse_uci_move(board: &Board, uci: &str) -> Option<Move> {
    if uci.len() < 4 {
        return None;
    }
    let chars: Vec<char> = uci.chars().collect();
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
        use z_slon::board::Piece;
        let white = board.is_white_to_move();
        let p = match chars[4] {
            'q' => Some(if white { Piece::WQueen } else { Piece::BQueen }),
            'r' => Some(if white { Piece::WRook } else { Piece::BRook }),
            'b' => Some(if white {
                Piece::WBishop
            } else {
                Piece::BBishop
            }),
            'n' => Some(if white {
                Piece::WKnight
            } else {
                Piece::BKnight
            }),
            _ => None,
        }?;
        Some(p)
    } else {
        None
    };

    let legal = legal_moves(board);
    legal
        .into_iter()
        .find(|m| m.from == from && m.to == to && m.promotion == promotion)
}

fn assert_legal_uci(board: &Board, uci: &str) {
    let mv = parse_uci_move(board, uci)
        .unwrap_or_else(|| panic!("illegal move {uci} in board\n{board}"));
    let mut b = board.clone();
    apply_move(&mut b, mv);
}

fn startpos() -> Board {
    Board::from_fen("rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1")
}

#[test]
fn ponderhit_returns_legal_reply_for_position_after_expected_move() {
    let (_child, mut stdin, mut reader) = spawn_engine();

    send(&mut stdin, "uci");
    send(&mut stdin, "isready");
    send(&mut stdin, "setoption name Ponder value true");
    send(&mut stdin, "ucinewgame");
    send(&mut stdin, "position startpos");

    send(&mut stdin, "go movetime 150");
    let bm_line = read_bestmove(&mut reader, Duration::from_secs(5));
    let (our_move, ponder_move) = parse_bestmove(&bm_line);

    let mut b = startpos();
    assert_legal_uci(&b, &our_move);
    let mv0 = parse_uci_move(&b, &our_move).unwrap();
    apply_move(&mut b, mv0);

    send(&mut stdin, &format!("position startpos moves {our_move}"));
    send(&mut stdin, "go ponder wtime 60000 btime 60000");

    let Some(their_move) = ponder_move else {
        send(&mut stdin, "stop");
        send(&mut stdin, "quit");
        return;
    };

    assert_legal_uci(&b, &their_move);

    send(
        &mut stdin,
        &format!("position startpos moves {our_move} {their_move}"),
    );
    send(&mut stdin, "ponderhit");

    let reply_line = read_bestmove(&mut reader, Duration::from_secs(5));
    let (reply, _next_ponder) = parse_bestmove(&reply_line);

    let mut after = b.clone();
    let mv1 = parse_uci_move(&after, &their_move).unwrap();
    apply_move(&mut after, mv1);

    assert_legal_uci(&after, &reply);

    send(&mut stdin, "quit");
}

#[test]
fn ponderhit_never_outputs_illegal_move_even_if_position_changes() {
    let (_child, mut stdin, mut reader) = spawn_engine();

    send(&mut stdin, "uci");
    send(&mut stdin, "isready");
    send(&mut stdin, "setoption name Ponder value true");
    send(&mut stdin, "ucinewgame");
    send(&mut stdin, "position startpos");

    send(&mut stdin, "go movetime 120");
    let bm_line = read_bestmove(&mut reader, Duration::from_secs(5));
    let (our_move, _ponder_move) = parse_bestmove(&bm_line);

    send(&mut stdin, &format!("position startpos moves {our_move}"));
    send(&mut stdin, "go ponder wtime 60000 btime 60000");

    send(&mut stdin, "position startpos moves e2e4");
    send(&mut stdin, "ponderhit");

    let reply_line = read_bestmove(&mut reader, Duration::from_secs(5));
    let (reply, _next_ponder) = parse_bestmove(&reply_line);

    let mut b = startpos();
    let e2e4 = parse_uci_move(&b, "e2e4").unwrap();
    apply_move(&mut b, e2e4);

    assert_legal_uci(&b, &reply);

    send(&mut stdin, "quit");
}
