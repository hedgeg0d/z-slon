#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."

BASE=${1:?usage: scripts/sprt.sh <baseline-binary> [tc] [max-rounds]}
TC=${2:-10+0.1}
ROUNDS=${3:-5000}

if [ ! -x "$BASE" ]; then
    echo "baseline binary not found/executable: $BASE" >&2
    exit 1
fi

cargo build --release
NEW=target/release/z-slon

CONCURRENCY=$(( $(nproc) > 2 ? $(nproc) - 1 : 1 ))

exec cutechess-cli \
    -engine name=new cmd="$NEW" \
    -engine name=base cmd="$BASE" \
    -each proto=uci tc="$TC" option.Hash=64 timemargin=200 \
    -openings file=scripts/openings.pgn format=pgn order=random \
    -repeat -games 2 -rounds "$ROUNDS" \
    -sprt elo0=0 elo1=5 alpha=0.05 beta=0.05 \
    -concurrency "$CONCURRENCY" \
    -ratinginterval 20 \
    -draw movenumber=40 movecount=8 score=10 \
    -resign movecount=4 score=600 \
    -recover
