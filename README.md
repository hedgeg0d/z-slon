# z-slon Chess Engine

A modern chess engine written in Rust featuring NNUE evaluation, opening books, and advanced search techniques.

## Features

### Core Engine
- **NNUE Evaluation**: Neural network evaluation using [timecat](https://crates.io/crates/timecat) with fallback to hand-crafted evaluation
- **Opening Books**: Polyglot book support via [polyglot-book-rs](https://crates.io/crates/polyglot-book-rs) crate
- **Advanced Search**: Multi-threaded alpha-beta with transposition table, move ordering, and pruning techniques
- **UCI Protocol**: Full UCI compliance with configurable options
- **Interactive CLI**: Real-time analysis with continuous evaluation mode

### Search Features
- Iterative deepening with aspiration windows
- Transposition table with replacement strategy
- Move ordering (hash move, captures, killers, history heuristic)
- Futility pruning and reverse futility pruning
- Quiescence search with delta pruning
- Multi-threaded parallel search

### Evaluation
- **NNUE**: Neural network evaluation when loaded
- **HCE Fallback**: Hand-crafted evaluation with material, PST, mobility, king safety
- Position-specific evaluation caching

## Building

```bash
cargo build --release
```

The optimized binary will be at `./target/release/z-slon`

## Usage

### Command Line Options

```bash
./target/release/z-slon [OPTIONS]

Options:
    --uci                Enable UCI mode
    --nnue <path>        Load NNUE evaluation file
    --book <path>        Load Polyglot opening book
    --threads <n>        Set number of search threads (default: 1)
    --debug              Enable debug output
```

### Interactive CLI Mode (Default)

```bash
./target/release/z-slon
```

#### CLI Commands

- `start eval` - Start continuous evaluation
- `stop` - Stop evaluation
- `move <move>` - Make a move (e.g., `move e2e4`)
- `show board` - Display current position
- `show moves` - Show all legal moves
- `show full` - Show board, moves, and game info
- `eval` - Show static evaluation
- `set depth <n>` - Set search depth (1-100)
- `set threads <n>` - Set thread count (1-64)
- `set board <fen>` - Set position from FEN
- `exit` or `quit` - Exit the engine

#### Example CLI Session

```
>> start eval
depth 1: best move e2e4
depth 2: best move d2d4
depth 3: best move d2d4
depth 4: best move e2e4
Evaluation: +20 (white advantage)
Breakdown:
Material: 0
PST: 10
Center: 10
Mobility: 0
King safety: 0

>> move e2e4
Move e2e4 applied.
depth 1: best move e7e5
depth 2: best move d7d5
...

>> stop
Stopping eval...

>> show board
8 r n b q k b n r 
7 p p p p p p p p 
6 . . . . . . . . 
5 . . . . . . . . 
4 . . . . P . . . 
3 . . . . . . . . 
2 P P P P . P P P 
1 R N B Q K B N R 
  a b c d e f g h
```

### UCI Mode (For Chess GUIs)

```bash
./target/release/z-slon --uci
```

#### UCI Options

- **Hash** (1-1024 MB): Transposition table size (default: 16)
- **Threads** (1-64): Number of search threads (default: 1)  
- **Book** (string): Path to Polyglot opening book file

#### UCI Commands

```bash
# Load NNUE evaluation
use nnue /path/to/model.nnue

# Set options
setoption name Threads value 4
setoption name Hash value 64
setoption name Book value /path/to/book.bin

# Quick test
echo -e "uci\nposition startpos\ngo depth 5\nquit" | ./target/release/z-slon --uci
```

## Using with Chess GUIs

### Arena Chess
1. Download Arena from http://www.playwitharena.de/
2. Engines → Install New Engine
3. Select `z-slon` executable
4. Choose UCI protocol

### Cutechess-cli
```bash
cutechess-cli \
  -engine cmd=./target/release/z-slon args="--uci --nnue model.nnue --book book.bin" proto=uci \
  -engine cmd=stockfish proto=uci \
  -each tc=40/60 \
  -rounds 10
```

### Lichess Bot
Compatible with [lichess-bot](https://github.com/lichess-bot-devs/lichess-bot)

## NNUE Evaluation

z-slon supports NNUE (Efficiently Updatable Neural Network) evaluation via the [timecat](https://crates.io/crates/timecat) crate:

```bash
# Load NNUE file at startup
./target/release/z-slon --nnue main_sf17.nnue

# Or load via UCI
use nnue /path/to/model.nnue
```

When NNUE is loaded, the engine automatically uses neural network evaluation. If no NNUE is loaded or loading fails, it falls back to hand-crafted evaluation (HCE).

## Opening Books

z-slon supports Polyglot opening books via the [polyglot-book-rs](https://crates.io/crates/polyglot-book-rs) crate:

```bash
# Load book at startup  
./target/release/z-slon --book Perfect2023.bin

# Or via UCI option
setoption name Book value /path/to/book.bin
```

The engine will automatically play book moves when available, with a preference for higher-weighted moves.

## Performance

The engine is optimized for speed with:
- Link-time optimization (LTO)
- Single codegen unit
- Native CPU instructions (`target-cpu=native`)
- Aggressive optimization level
- Stripped binaries

Typical performance on modern hardware: 200K-500K nodes/second (single thread)

## Architecture

### Search Algorithm
- **Iterative Deepening**: Progressive depth increase with time management
- **Alpha-Beta Pruning**: Minimax with cutoffs and aspiration windows
- **Transposition Table**: Position caching with zobrist hashing
- **Move Ordering**: Hash move → Captures (MVV-LVA) → Killers → History heuristic
- **Pruning Techniques**: Futility, reverse futility, razoring, delta pruning
- **Quiescence Search**: Tactical search to avoid horizon effect
- **Parallel Search**: Multi-threaded search with shared transposition table

### Evaluation System
- **NNUE Primary**: Neural network evaluation via timecat integration
- **HCE Fallback**: Material, piece-square tables, mobility, king safety
- **Evaluation Caching**: Position-specific evaluation storage

### Move Generation
- Bitboard-based board representation
- Legal move generation with check detection
- Special moves: castling, en passant, promotion
- Efficient move application and undo

## Project Structure

```
src/
├── main.rs                   # CLI interface and main entry point
├── uci.rs                    # UCI protocol implementation  
├── board.rs                  # Board representation and FEN parsing
├── movegen.rs                # Move generation and application
├── search.rs                 # Advanced search with transposition table
├── eval.rs                   # Evaluation (NNUE + HCE fallback)
├── nnue.rs                   # NNUE evaluation wrapper
└── polyglot_integration.rs   # Polyglot book integration
```

## Development

### Run Tests
```bash
cargo test
```

### Run in Debug Mode
```bash
cargo run -- --debug
```

### Profile Performance
```bash
cargo build --release
perf record ./target/release/z-slon --uci < test_commands.txt
perf report
```

## Dependencies

- **[timecat](https://crates.io/crates/timecat)**: NNUE evaluation support
- **[polyglot-book-rs](https://crates.io/crates/polyglot-book-rs)**: Polyglot opening book support  
- **[tokio](https://crates.io/crates/tokio)**: Async runtime for concurrent operations
- **[rustyline](https://crates.io/crates/rustyline)**: Interactive CLI with readline support
- **[clap](https://crates.io/crates/clap)**: Command-line argument parsing

## License

MIT License - See LICENSE file for details

## Author

hedgegod

## Contributing

Contributions welcome! Areas for improvement:
- Endgame tablebases (Syzygy)
- Advanced time management
- Pondering (thinking on opponent's time)
- Multi-PV search
- Lazy SMP improvements
- Additional pruning techniques
