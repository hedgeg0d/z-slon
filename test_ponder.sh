#!/bin/bash

# Test script for pondering chains in z-slon

# Colors for output
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m' # No Color

echo "Testing Pondering Chains in z-slon..."
echo "======================================"

# Build the engine if needed
if [ ! -f "target/release/z-slon" ]; then
    echo "Building z-slon..."
    cargo build --release
fi

# Create a UCI test file
cat > ponder_test.uci << 'EOF'
# UCI pondering chain test
uci
isready
ucinewgame
position startpos
go ponder
# Wait for ponder output
# Send ponderhit when we see a ponder move
EOF

# Function to test pondering chain
test_ponder_chain() {
    echo "Starting ponder chain test..."
    
    # Start the engine in UCI mode
    ./target/release/z-slon > engine_output.log 2>&1 &
    ENGINE_PID=$!
    
    # Give it time to start
    sleep 1
    
    # Send UCI commands
    echo "uci" | nc localhost 9999 2>/dev/null || echo "uci" > /proc/$ENGINE_PID/fd/0 2>/dev/null
    
    # Wait for engine to respond
    sleep 2
    
    # Check if engine output contains ponder moves
    if grep -q "ponder" engine_output.log; then
        echo -e "${GREEN}✓ Engine is outputting ponder moves${NC}"
        
        # Look for bestmove + ponder pattern
        if grep -E "bestmove [a-h][1-8][a-h][1-8][qrnb]? ponder [a-h][1-8][a-h][1-8][qrnb]?" engine_output.log; then
            echo -e "${GREEN}✓ Found bestmove + ponder output${NC}"
            return 0
        else
            echo -e "${RED}✗ No bestmove + ponder pattern found${NC}"
            return 1
        fi
    else
        echo -e "${RED}✗ No ponder output found${NC}"
        return 1
    fi
}

# Test 1: Basic ponder output
echo "Test 1: Checking if engine outputs bestmove + ponder..."
echo "position startpos" | ./target/release/z-slon --cli 2>&1 | grep -E "(bestmove|ponder)" || echo "CLI mode doesn't use UCI protocol"

# Test 2: UCI protocol test
echo -e "\nTest 2: UCI protocol pondering test..."

# Create a more comprehensive UCI test
timeout 30s bash -c '
    ./target/release/z-slon > /tmp/engine.log 2>&1 &
    PID=$!
    sleep 1
    
    # Send UCI commands
    echo "uci" > /proc/$PID/fd/0 2>/dev/null
    sleep 1
    echo "isready" > /proc/$PID/fd/0 2>/dev/null
    sleep 1
    echo "ucinewgame" > /proc/$PID/fd/0 2>/dev/null
    sleep 1
    echo "position startpos" > /proc/$PID/fd/0 2>/dev/null
    sleep 1
    echo "go ponder" > /proc/$PID/fd/0 2>/dev/null
    sleep 3
    
    # Check log
    if [ -f /tmp/engine.log ]; then
        echo "Engine output:"
        cat /tmp/engine.log
        if grep -q "bestmove.*ponder" /tmp/engine.log; then
            echo "SUCCESS: Found bestmove + ponder!"
        else
            echo "FAILURE: No bestmove + ponder found"
        fi
    fi
    
    kill $PID 2>/dev/null
'

# Cleanup
rm -f ponder_test.uci engine_output.log

echo -e "\nTest completed."
