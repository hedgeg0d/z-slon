#!/bin/bash
# Simple test for pondering chains

echo "Testing Pondering Chains..."
echo "=========================="

# Create UCI command sequence
cat > commands.txt << 'EOF'
uci
isready
ucinewgame
position startpos
go depth 10
quit
EOF

echo "Step 1: Testing basic bestmove+ponder output..."
timeout 30s ./target/release/z-slon < commands.txt 2>&1 | tee engine_output.log

echo ""
echo "Checking for bestmove + ponder in output..."
if grep -E "bestmove [a-h][1-8][a-h][1-8].*ponder [a-h][1-8][a-h][1-8]" engine_output.log; then
    echo "✅ SUCCESS: Engine outputs bestmove + ponder"
else
    echo "❌ FAILURE: No bestmove + ponder found"
    echo "Searching for any bestmove output..."
    grep "bestmove" engine_output.log || echo "No bestmove found at all"
fi

# Cleanup
rm -f commands.txt engine_output.log
