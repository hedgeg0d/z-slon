# z-slon Chess Engine

A modern chess engine written in Rust featuring embedded NNUE evaluation, opening books, and advanced search techniques.

## 0.8.0

- Correct root TT bounds for aspiration-window fail-high/fail-low results.
- Avoid unrestricted root bounds from restricted MultiPV searches and
  incomplete bounds from interrupted searches; fix quiescence stand-pat bounds.
- Keep move-derived incremental NNUE updates via `nnue-rs 0.4.1`. Lazy-update
  experiments were not retained because paired benchmarks showed no reliable gain.
- Validation: 51 unit tests and UCI lifecycle checks passed. A bounded `8+0.08`
  match scored 9 wins, 7 losses and 12 draws, without an SPRT verdict. This does
  not establish an Elo gain or exclude a strength regression.

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
- Multi-threaded parallel search (Lazy SMP)
- **Pondering**: Think during opponent's time (competition-ready)

### Evaluation
- **NNUE**: Neural network evaluation when loaded
- **HCE Fallback**: Hand-crafted evaluation with material, PST, mobility, king safety
- Position-specific evaluation caching

## Building

The engine depends on `nnue-rs 0.4.1` from crates.io for the incremental
`update_changes` API. Local crate checkouts are not part of this repository.

```bash
cargo build --release
```

The default build uses `target-cpu=native`; its CPU instruction requirements
match the build machine. Build on each target device, or use a generic build
for distribution within the same architecture:

```bash
RUSTFLAGS="" cargo build --release
```

The ultra-optimized binary will be at `./target/release/z-slon` (~94MB, includes 72MB embedded NNUE)

**Build optimizations:**
- Maximum optimization level (opt-level = 3)
- Link-time optimization (LTO = fat)
- Single codegen unit for best performance
- Stripped symbols for minimal size
- Abort on panic for smaller binary

## Benchmarking

```bash
./target/release/z-slon --bench        # depth 10 over 14 fixed positions
./target/release/z-slon --bench 12     # deeper run
```

The benchmark is fully deterministic (single thread, fresh TT per position,
no timers): `Nodes searched` must be **bit-identical** between binaries with
identical search logic. This makes it a regression gate:

- **performance-only patches** (movegen, magics, make/unmake): nodes must not
  change, `Nodes/second` shows the speedup
- **search patches**: nodes will change — that is expected; compare strength
  with SPRT instead

## Strength Testing (SPRT)

Strength matches can use an external UCI runner such as
[fastchess](https://github.com/Disservin/fastchess). Match runners, opening
suites, downloads and tournament outputs are not bundled with the engine.

Methodology:

- **both binaries must use the same NNUE** (both with the embedded network) —
  otherwise the test measures "patch + net" together
- one change per test, never two patches in one binary
- STC fast triage first, LTC confirmation only for promising search patches
- Compare time managers using the same binary with the UCI option
  `Time Management` set to `adaptive` and `linear`, respectively.
- Time-manager tests need both increment and no-increment controls, e.g.
  triage at `20+0.2`, confirmation at `60+0.6` and `300+0`. Also check time
  forfeits/latency and multiple thread counts; a short smoke match proves no Elo gain.

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
    --nnue <path>        Legacy option; currently leaves embedded NNUE unchanged
    --book <path>        Load Polyglot opening book
    --threads <n>        Set number of search threads (default: 1)
    --debug              Enable debug output
    --bench [depth]      Run deterministic benchmark and exit (default depth: 10)
    --version            Print version and exit
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
- **EvalFile** (string): Export destination for embedded NNUE, or `<embedded>` (default: `<embedded>`). Does not load external networks.
- **Book** (string): Path to Polyglot opening book file
- **Ponder** (check): Pondering support - think during opponent's time (default: false)
- **MultiPV** (1-500): Multiple principal variations (default: 1)
- **UCI_Chess960** (check): Chess960 mode (not yet implemented)
- **Move Overhead** (0-5000 ms): Clock reserve for scheduling, I/O and GUI latency (default: 30 ms)
- **Time Management** (`linear` / `adaptive`): Defaults to `adaptive`: existing
  linear allocation, soft stop after a completed iteration, hard limit up to
  three times that allocation (still clock-capped). No phase or instability
  heuristics yet. `linear` retains the previous hard-stop policy for A/B tests
  and rollback. Adoption is based on a positive preliminary self-play result,
  not a completed SPRT pass; longer and no-increment confirmation remain needed.
- **nodestime** (0-10000): Nodes per second (not yet implemented)

#### UCI Commands

Clock budgets use the side-to-move clock and honor `movestogo`. `movetime`
is a fixed hard limit, independent of the Time Management mode; `infinite`
has no clock budget. An explicit `depth` remains a depth cap even with clocks.
Pondering has no active deadline until `ponderhit`; the full budget starts at
that hit, not at the beginning of pondering. Search-local cancellation prevents
an old timer from stopping a later search. Set Move Overhead for your device:
wall-clock deadlines cannot eliminate OS scheduling delays.

Build and run the engine's built-in unit tests:

```bash
timeout 300 cargo build --release
timeout 180 cargo test --release
```

```bash
# Set options
setoption name Threads value 8
setoption name Hash value 256
setoption name Book value /path/to/book.bin

# Export embedded NNUE (like Stockfish)
setoption name EvalFile value /tmp/exported.nnue

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

**Warning:** `EvalFile` writes the embedded network to the specified path and
overwrites an existing file. It is an export operation, not a loader.
External-network loading is not implemented; the legacy `--nnue` option does
not replace the embedded network.

The engine uses [nnue-rs](https://crates.io/crates/nnue-rs) for incremental
evaluation. If the embedded network cannot be parsed, it falls back to
hand-crafted evaluation (HCE).

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

Performance depends on hardware and position. Use `--bench` to measure your
device; NPS is not an Elo measurement.

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

- **[nnue-rs](https://crates.io/crates/nnue-rs)**: NNUE loading, evaluation and incremental accumulator updates
- **[polyglot-book-rs](https://crates.io/crates/polyglot-book-rs)**: Polyglot opening book support  
- **[tokio](https://crates.io/crates/tokio)**: Async runtime for responsive concurrent operations
- **[rustyline](https://crates.io/crates/rustyline)**: Interactive CLI with readline support
- **[clap](https://crates.io/crates/clap)**: Command-line argument parsing
- **[lazy_static](https://crates.io/crates/lazy_static)**: Static initialization for FFI bindings

## Lichess Integration

Run the engine as an ordinary UCI executable from an external
[lichess-bot](https://github.com/lichess-bot-devs/lichess-bot) installation.
Launchers, bot configurations and API tokens are managed outside this
repository; none are required to build or run the engine locally.

## License

MIT License - See LICENSE file for details

## Author

hedgegod

## Contributing

Contributions welcome! Areas for improvement:
- Endgame tablebases (Syzygy)
- Advanced time management
- Lazy SMP improvements
- Additional pruning techniques
- UCI_Chess960 support
