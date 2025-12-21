#!/bin/bash
# Debug the search output

echo "Debugging search output..."

# Create a simple position and search
cat > debug_cmds.txt << 'EOF'
uci
isready
ucinewgame
position startpos
go depth 5
quit
EOF

timeout 10s ./target/release/z-slon < debug_cmds.txt 2>&1 | tee debug_output.log

echo ""
echo "Analyzing output..."
grep -E "(info|bestmove|ponder|depth|pv)" debug_output.log | tail -20

rm -f debug_cmds.txt
