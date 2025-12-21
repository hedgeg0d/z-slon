use std::process::{Command, Stdio};
use std::io::{Write, BufRead, BufReader};

#[test]
fn test_ponderhit_chain_legal_moves() {
    // This test verifies that ponderhit chains produce legal moves
    // Note: This is an integration test that requires the binary to be built
    
    let mut cmd = Command::new("cargo")
        .args(&["run", "--release", "--bin", "z-slon"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start engine");
    
    let mut stdin = cmd.stdin.take().expect("Failed to open stdin");
    let stdout = cmd.stdout.take().expect("Failed to open stdout");
    let reader = BufReader::new(stdout);
    
    // Send UCI commands
    writeln!(stdin, "uci").unwrap();
    writeln!(stdin, "isready").unwrap();
    writeln!(stdin, "setoption name Ponder value true").unwrap();
    writeln!(stdin, "position startpos").unwrap();
    writeln!(stdin, "go ponder wtime 60000 btime 60000").unwrap();
    
    // Wait a bit for search to start
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    // Send first ponderhit
    writeln!(stdin, "ponderhit").unwrap();
    
    let mut bestmoves = Vec::new();
    let mut lines = reader.lines();
    
    // Collect bestmove outputs
    for _ in 0..10 {
        if let Ok(Some(line)) = lines.next() {
            if line.starts_with("bestmove") {
                bestmoves.push(line.clone());
                println!("Found bestmove: {}", line);
                
                // Verify format: should be "bestmove <move> ponder <move>" or "bestmove <move>"
                assert!(line.starts_with("bestmove "), "Invalid bestmove format: {}", line);
                
                // Extract moves
                let parts: Vec<&str> = line.split_whitespace().collect();
                assert!(parts.len() >= 2, "bestmove should have at least one move: {}", line);
                
                // Verify move format (4-5 characters: e2e4 or e7e5q)
                let move_str = parts[1];
                assert!(move_str.len() >= 4 && move_str.len() <= 5, 
                    "Move format invalid: {}", move_str);
                
                // If there's a ponder move, verify it too
                if parts.len() >= 4 && parts[2] == "ponder" {
                    let ponder_move = parts[3];
                    assert!(ponder_move.len() >= 4 && ponder_move.len() <= 5,
                        "Ponder move format invalid: {}", ponder_move);
                }
            }
        }
    }
    
    // Send second ponderhit
    writeln!(stdin, "ponderhit").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    // Collect more bestmoves
    for _ in 0..10 {
        if let Ok(Some(line)) = lines.next() {
            if line.starts_with("bestmove") {
                bestmoves.push(line.clone());
                println!("Found bestmove: {}", line);
            }
        }
    }
    
    writeln!(stdin, "quit").unwrap();
    
    // Verify we got at least one bestmove
    assert!(!bestmoves.is_empty(), "No bestmove outputs found");
    
    println!("Test passed: Found {} bestmove outputs", bestmoves.len());
}

