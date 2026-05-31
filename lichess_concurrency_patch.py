import sys
from pathlib import Path

bot_dir = Path(sys.argv[1] if len(sys.argv) > 1 else Path.home() / "lichess-bot")

PATCHES = [
    (
        bot_dir / "lib" / "matchmaking.py",
        "ongoing_now = self.li.get_ongoing_games()",
        "        max_games_for_matchmaking = max_games if self.matchmaking_cfg.allow_during_games else min(1, max_games)\n"
        "        game_count = len(active_games) + len(challenge_queue)",
        "        max_games_for_matchmaking = max_games if self.matchmaking_cfg.allow_during_games else min(1, max_games)\n"
        "        ongoing_now = self.li.get_ongoing_games() or []\n"
        "        game_count = max(len(active_games), len(ongoing_now)) + len(challenge_queue)",
    ),
    (
        bot_dir / "lib" / "lichess_bot.py",
        "aborting extra game",
        'def start_game_thread(active_games: set[str], game_id: str, play_game_args: PlayGameArgsType, pool: POOL_TYPE) -> None:\n'
        '    """Start a game thread."""\n'
        '    active_games.add(game_id)\n'
        '    log_proc_count("Used", active_games)',
        'def start_game_thread(active_games: set[str], game_id: str, play_game_args: PlayGameArgsType, pool: POOL_TYPE) -> None:\n'
        '    """Start a game thread."""\n'
        '    if game_id not in active_games:\n'
        '        max_games = play_game_args["config"].challenge.concurrency\n'
        '        li = play_game_args["li"]\n'
        '        ongoing = li.get_ongoing_games() or []\n'
        '        others = [game for game in ongoing if game.get("gameId") != game_id]\n'
        '        if len(others) >= max_games:\n'
        '            logger.warning(f"Over concurrency limit ({max_games}); aborting extra game {game_id}.")\n'
        '            try:\n'
        '                li.abort(game_id)\n'
        '                return\n'
        '            except Exception:\n'
        '                logger.exception(f"Could not abort {game_id}; playing it to avoid a time loss.")\n'
        '    active_games.add(game_id)\n'
        '    log_proc_count("Used", active_games)',
    ),
]

applied, already, failed = [], [], []
for path, marker, old, new in PATCHES:
    if not path.exists():
        failed.append(f"{path.name}: missing")
        continue
    text = path.read_text()
    if marker in text:
        already.append(path.name)
    elif old in text:
        path.write_text(text.replace(old, new))
        applied.append(path.name)
    else:
        failed.append(f"{path.name}: anchor not found (upstream changed?)")

if applied:
    print("concurrency patch applied:", ", ".join(applied))
if already:
    print("concurrency patch already present:", ", ".join(already))
if failed:
    print("WARNING concurrency patch could not apply:", "; ".join(failed))
    sys.exit(1)
