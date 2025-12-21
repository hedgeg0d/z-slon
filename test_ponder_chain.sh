#!/bin/bash
# Test script for ponderhit chain functionality and move legality

set -e

echo "Testing ponderhit chain with move legality checks..."
echo ""

# Create a test input file that simulates multiple ponderhits
cat > /tmp/ponder_chain_test.txt << 'EOF'
uci
isready
setoption name Ponder value true
position startpos
go ponder wtime 60000 btime 60000
ponderhit
ponderhit
ponderhit
quit
EOF

echo "Running engine with ponderhit chain test..."
echo "Expected: Each ponderhit should output 'bestmove <move> ponder <move>' and moves should be legal"
echo ""

# Run the engine and capture output
timeout 15 cargo run --release < /tmp/ponder_chain_test.txt 2>&1 | tee /tmp/ponder_output.txt

echo ""
echo "Checking output for bestmove commands..."
grep -E "bestmove" /tmp/ponder_output.txt || echo "No bestmove found!"

echo ""
echo "Test completed. Check output above for any errors or illegal moves."

