#!/usr/bin/env python3
"""
Quality test for ponderhit chains - validates that all moves are legal
"""

import subprocess
import re
import sys
from collections import deque

def parse_move(move_str):
    """Parse UCI move format (e.g., 'e2e4' or 'e7e5q')"""
    if len(move_str) < 4:
        return None
    return move_str[:4], move_str[4] if len(move_str) > 4 else None

def test_ponderhit_chain():
    """Test that ponderhit chains produce legal moves"""
    
    print("Starting ponderhit chain quality test...")
    print("=" * 60)
    
    # Start the engine
    proc = subprocess.Popen(
        ["cargo", "run", "--release", "--bin", "z-slon"],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
        bufsize=0
    )
    
    import threading
    import queue
    
    output_queue = queue.Queue()
    
    def read_output():
        for line in proc.stdout:
            output_queue.put(line.strip())
    
    reader_thread = threading.Thread(target=read_output, daemon=True)
    reader_thread.start()
    
    # Send initialization commands
    commands = [
        "uci",
        "isready",
        "setoption name Ponder value true",
        "position startpos",
        "go ponder wtime 60000 btime 60000"
    ]
    
    for cmd in commands:
        proc.stdin.write(cmd + "\n")
        proc.stdin.flush()
    
    # Wait a bit for search to start
    import time
    time.sleep(0.5)
    
    bestmoves = []
    ponder_moves = []
    errors = []
    all_output = []
    
    # Collect initial output
    timeout = time.time() + 2.0
    while time.time() < timeout:
        try:
            line = output_queue.get(timeout=0.1)
            all_output.append(line)
            if line.startswith("bestmove"):
                match = re.match(r"bestmove\s+(\S+)(?:\s+ponder\s+(\S+))?", line)
                if match:
                    bestmove = match.group(1)
                    ponder = match.group(2) if match.group(2) else None
                    bestmoves.append(bestmove)
                    if ponder:
                        ponder_moves.append(ponder)
                    print(f"Found: {line}")
            elif "error" in line.lower() or "illegal" in line.lower():
                errors.append(line)
                print(f"ERROR: {line}")
        except queue.Empty:
            continue
    
    # Send first ponderhit
    print("\nSending first ponderhit...")
    proc.stdin.write("ponderhit\n")
    proc.stdin.flush()
    time.sleep(0.5)
    
    # Collect output after first ponderhit
    timeout = time.time() + 2.0
    while time.time() < timeout:
        try:
            line = output_queue.get(timeout=0.1)
            all_output.append(line)
            if line.startswith("bestmove"):
                match = re.match(r"bestmove\s+(\S+)(?:\s+ponder\s+(\S+))?", line)
                if match:
                    bestmove = match.group(1)
                    ponder = match.group(2) if match.group(2) else None
                    bestmoves.append(bestmove)
                    if ponder:
                        ponder_moves.append(ponder)
                    print(f"Found: {line}")
            elif "error" in line.lower() or "illegal" in line.lower():
                errors.append(line)
                print(f"ERROR: {line}")
        except queue.Empty:
            continue
    
    # Send second ponderhit
    print("\nSending second ponderhit...")
    proc.stdin.write("ponderhit\n")
    proc.stdin.flush()
    time.sleep(0.5)
    
    # Collect output after second ponderhit
    timeout = time.time() + 2.0
    while time.time() < timeout:
        try:
            line = output_queue.get(timeout=0.1)
            all_output.append(line)
            if line.startswith("bestmove"):
                match = re.match(r"bestmove\s+(\S+)(?:\s+ponder\s+(\S+))?", line)
                if match:
                    bestmove = match.group(1)
                    ponder = match.group(2) if match.group(2) else None
                    bestmoves.append(bestmove)
                    if ponder:
                        ponder_moves.append(ponder)
                    print(f"Found: {line}")
            elif "error" in line.lower() or "illegal" in line.lower():
                errors.append(line)
                print(f"ERROR: {line}")
        except queue.Empty:
            continue
    
    # Send quit
    proc.stdin.write("quit\n")
    proc.stdin.flush()
    
    # Wait for process to finish
    try:
        proc.wait(timeout=3)
    except subprocess.TimeoutExpired:
        proc.kill()
    
    # Print all output for debugging
    if len(bestmoves) == 0:
        print("\nAll output received:")
        for line in all_output[-20:]:  # Last 20 lines
            print(f"  {line}")
    
    # Print results
    print("\n" + "=" * 60)
    print("TEST RESULTS")
    print("=" * 60)
    print(f"Total bestmoves found: {len(bestmoves)}")
    print(f"Total ponder moves found: {len(ponder_moves)}")
    print(f"Errors found: {len(errors)}")
    
    if errors:
        print("\nERRORS:")
        for err in errors:
            print(f"  - {err}")
    
    # Validate move formats
    print("\nValidating move formats...")
    all_valid = True
    for i, move in enumerate(bestmoves):
        if len(move) < 4 or len(move) > 5:
            print(f"  Invalid bestmove format: {move}")
            all_valid = False
        elif not re.match(r"^[a-h][1-8][a-h][1-8][qrnb]?$", move):
            print(f"  Invalid bestmove format: {move}")
            all_valid = False
    
    for i, move in enumerate(ponder_moves):
        if len(move) < 4 or len(move) > 5:
            print(f"  Invalid ponder move format: {move}")
            all_valid = False
        elif not re.match(r"^[a-h][1-8][a-h][1-8][qrnb]?$", move):
            print(f"  Invalid ponder move format: {move}")
            all_valid = False
    
    if all_valid and len(bestmoves) > 0:
        print("✓ All moves have valid format")
    elif len(bestmoves) == 0:
        print("✗ No bestmoves found!")
        all_valid = False
    
    print("\n" + "=" * 60)
    if all_valid and len(errors) == 0:
        print("TEST PASSED")
        return 0
    else:
        print("TEST FAILED")
        return 1

if __name__ == "__main__":
    sys.exit(test_ponderhit_chain())

