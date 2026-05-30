#!/bin/bash
# SPRT self-play: z-slon-new vs z-slon-old
# Usage: ./sprt.sh [TC] [elo1]   e.g. ./sprt.sh 8+0.08 10
set -e
cd "$(dirname "$0")"

TC="${1:-8+0.08}"        # time+increment seconds per game
ELO1="${2:-10}"          # H1 elo bound (test passes if gain >= this)
HASH=64
THREADS=1
CONCURRENCY=6            # 12 cores / 2 engines (1 thread each)
BOOK=8moves_v3.pgn

NEW="$PWD/z-slon-new"
OLD="$PWD/z-slon-old"
OLD_NNUE="$PWD/nn-37f18f62d772.nnue"

cutechess-cli \
  -engine name=new cmd="$NEW" \
      option.Threads=$THREADS option.Hash=$HASH \
  -engine name=old cmd="$OLD" arg=--nnue arg="$OLD_NNUE" \
      option.Threads=$THREADS option.Hash=$HASH \
  -each proto=uci tc=$TC \
  -openings file=$BOOK format=pgn order=random -repeat -games 2 \
  -concurrency $CONCURRENCY \
  -sprt elo0=0 elo1=$ELO1 alpha=0.05 beta=0.05 \
  -resign movecount=3 score=600 -draw movenumber=40 movecount=8 score=10 \
  -ratinginterval 10 -pgnout sprt_games.pgn
