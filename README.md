# z-slon Chess Engine

A modern chess engine written in Rust featuring embedded NNUE evaluation, opening books, and advanced search techniques.

## Features

### Core Engine
- **Embedded NNUE**: 72MB Stockfish 17 NNUE network packed into the binary for zero-config evaluation
- **UCI-First Design**: UCI mode by default for seamless integration with chess GUIs
- **Opening Books**: Polyglot book support via [polyglot-book-rs](https://crates.io/crates/polyglot-book-rs) crate
- **Advanced Search**: Multi-threaded alpha-beta with transposition table, move ordering, and pruning techniques
- **Responsive Search**: Can accept commands (like 'stop') during infinite search
- **Interactive CLI**: Real-time analysis with continuous evaluation mode (via `--cli` flag)

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

The ultra-optimized binary will be at `./target/release/z-slon` (~94MB, includes 72MB embedded NNUE)

**Build optimizations:**
- Maximum optimization level (opt-level = 3)
- Link-time optimization (LTO = fat)
- Single codegen unit for best performance
- Stripped symbols for minimal size
- Abort on panic for smaller binary

## Usage

### UCI Mode (Default - For Chess GUIs)

```bash
./target/release/z-slon
```

The engine starts in UCI mode automatically. Simply launch it from your chess GUI (Arena, ChessBase, Lichess, etc.).

**Command Line Options:**
```bash
./target/release/z-slon [OPTIONS]

Options:
    --cli                Enable interactive CLI mode
    --nnue <path>        Load external NNUE file (overrides embedded)
    --book <path>        Load Polyglot opening book
    --threads <n>        Set number of search threads (default: 1)
    --debug              Enable debug output
```

### Interactive CLI Mode

```bash
./target/release/z-slon --cli
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

### UCI Protocol

The engine runs in UCI mode by default and supports standard UCI commands.

#### Supported UCI Options

- **Hash** (1-33554432 MB): Transposition table size (default: 16)
- **Threads** (1-512): Number of search threads (default: 1)
- **EvalFile** (string): NNUE file path or `<embedded>` (default: `<embedded>`)
- **Book** (string): Path to Polyglot opening book file
- **UCI_Chess960** (check): Chess960 mode (not yet implemented)
- **Ponder** (check): Pondering support (not yet implemented)
- **MultiPV** (1-500): Multiple principal variations (not yet implemented)
- **Move Overhead** (0-5000 ms): Time management overhead (not yet implemented)
- **nodestime** (0-10000): Nodes per second (not yet implemented)

#### UCI Commands

```bash
# Set options
setoption name Threads value 8
setoption name Hash value 256
setoption name Book value /path/to/book.bin

# Export embedded NNUE (like Stockfish)
setoption name EvalFile value /tmp/exported.nnue

# Load external NNUE
setoption name EvalFile value /path/to/custom.nnue

# Quick test
echo -e "uci\nposition startpos\ngo depth 5\nquit" | ./target/release/z-slon

# Test infinite search with stop
echo -e "position startpos\ngo infinite" | ./target/release/z-slon &
sleep 3
echo "stop"
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
  -engine cmd=./target/release/z-slon proto=uci \
  -engine cmd=stockfish proto=uci \
  -each tc=40/60 \
  -rounds 10
```

### Lichess Bot
Compatible with [lichess-bot](https://github.com/lichess-bot-devs/lichess-bot)

## NNUE Evaluation

z-slon has **main_sf17.nnue (72MB) embedded directly in the binary** and uses it automatically. No external files needed!

**Export the embedded NNUE (like Stockfish):**
```bash
echo "setoption name EvalFile value /tmp/my.nnue" | ./target/release/z-slon
```

**Load a different NNUE:**
```bash
# Via command line
./target/release/z-slon --nnue /path/to/custom.nnue

# Via UCI option
setoption name EvalFile value /path/to/custom.nnue
```

The engine uses NNUE evaluation via the [timecat](https://crates.io/crates/timecat) crate. If NNUE loading fails, it falls back to hand-crafted evaluation (HCE).

## Opening Books

z-slon supports Polyglot opening books:

```bash
# Load book at startup  
./target/release/z-slon --book Perfect2023.bin

# Or via UCI option
setoption name Book value /path/to/book.bin
```

The engine automatically plays book moves when available, preferring higher-weighted moves.

## Performance

**Ultra-optimized build settings:**
- Maximum optimization level (`opt-level = 3`)
- Fat link-time optimization (`lto = "fat"`)
- Single codegen unit (`codegen-units = 1`)
- Stripped symbols (`strip = true`)
- Abort on panic (`panic = "abort"`)

**Typical performance:** 100K-200K nodes/second (single thread) with NNUE evaluation

**Responsive during search:** The engine can receive and process commands (like `stop`) even during `go infinite`

## Architecture

### Search Algorithm
- **Iterative Deepening**: Progressive depth increase with time management
- **Alpha-Beta Pruning**: Minimax with cutoffs and aspiration windows
- **Transposition Table**: Position caching with zobrist hashing
- **Move Ordering**: Hash move → Captures (MVV-LVA) → Killers → History heuristic
- **Pruning Techniques**: Futility, reverse futility, razoring, delta pruning
- **Quiescence Search**: Tactical search to avoid horizon effect
- **Parallel Search**: Multi-threaded search with shared transposition table
- **Responsive Search**: Non-blocking search allows commands during `go infinite`

### Evaluation System
- **Embedded NNUE**: Stockfish 17 NNUE (72MB) packed into binary, loaded automatically
- **NNUE Export**: Can export embedded network like Stockfish
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
- **[tokio](https://crates.io/crates/tokio)**: Async runtime for responsive concurrent operations
- **[rustyline](https://crates.io/crates/rustyline)**: Interactive CLI with readline support
- **[clap](https://crates.io/crates/clap)**: Command-line argument parsing
- **[lazy_static](https://crates.io/crates/lazy_static)**: Static initialization for FFI bindings

## License

MIT License - See LICENSE file for details

## Author

hedgegod

## Recent Improvements (v0.3.0)

- ✅ **UCI by default**: Engine starts in UCI mode automatically (use `--cli` for interactive mode)
- ✅ **Embedded NNUE**: 72MB Stockfish 17 NNUE packed into binary, auto-loaded
- ✅ **NNUE export**: Can export embedded network via UCI option (like Stockfish)
- ✅ **Stockfish-like options**: Hash, Threads, EvalFile, Book, and more UCI options
- ✅ **Responsive search**: Commands like `stop` work during `go infinite`
- ✅ **Ultra-optimized build**: Fat LTO, single codegen unit, maximum optimization
- ✅ **Zero warnings**: Clean compilation
- ✅ **Single binary**: Only z-slon binary is built

## Contributing

Contributions welcome! Areas for improvement:
- Endgame tablebases (Syzygy)
- Advanced time management
- Pondering (thinking on opponent's time)
- Multi-PV search
- Lazy SMP improvements
- Additional pruning techniques
- UCI_Chess960 support
