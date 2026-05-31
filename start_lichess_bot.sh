#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BOT_DIR="${ZSLON_BOT_DIR:-$HOME/lichess-bot}"
BASE_CONFIG="$BOT_DIR/config_turbo.yml"
GEN_CONFIG="$BOT_DIR/config_zslon_active.yml"
TOKEN_FILE="$SCRIPT_DIR/.lichess_token"

MODE="matchmaking"
THREADS="10"
PONDER="on"
RATED="rated"
RATING_DIFF="300"
TCS="180+2 300+0 600+0"

C_RESET=$'\033[0m'; C_DIM=$'\033[2m'; C_CYAN=$'\033[36m'; C_GRN=$'\033[32m'; C_BOLD=$'\033[1m'

banner() {
cat <<EOF
${C_CYAN}${C_BOLD} ┌──────────────────────────────────────────────┐
 │   z-slon  ·  lichess launcher  ·  v0.6.0       │
 └──────────────────────────────────────────────┘${C_RESET}
 ${C_DIM}NNUE · pondering · ponderchain${C_RESET}
EOF
}

pick() {
    local header="$1"; shift
    printf '%s\n' "$@" | fzf --height=40% --reverse --border=rounded \
        --prompt="› " --pointer="▶" --header="$header" \
        --color='border:cyan,header:yellow,pointer:green,prompt:cyan,fg+:green:bold'
}

pick_multi() {
    local header="$1"; shift
    printf '%s\n' "$@" | fzf --multi --height=50% --reverse --border=rounded \
        --prompt="› " --pointer="▶" --marker="✓" \
        --header="$header  (TAB = mark, ENTER = confirm)" \
        --color='border:cyan,header:yellow,pointer:green,marker:magenta,prompt:cyan,fg+:green:bold'
}

edit_settings() {
    while true; do
        clear; banner; echo
        local menu=()
        menu+=("Mode          ❯ $MODE")
        menu+=("Threads       ❯ $THREADS")
        menu+=("Pondering     ❯ $PONDER")
        if [ "$MODE" = "matchmaking" ]; then
            menu+=("Game type     ❯ $RATED")
            menu+=("Rating range  ❯ ±$RATING_DIFF")
            menu+=("Time controls ❯ $TCS")
        fi
        menu+=("───────────────────────")
        menu+=("▶  START BOT")
        menu+=("✖  Quit")

        local sel
        sel=$(printf '%s\n' "${menu[@]}" | fzf --height=60% --reverse --border=rounded \
            --prompt="select › " --pointer="▶" \
            --header=$'configure z-slon, then START\nENTER on a row to change it' \
            --color='border:cyan,header:yellow,pointer:green,prompt:cyan,fg+:green:bold,hl+:green') || return 1
        [ -z "$sel" ] && return 1

        case "$sel" in
            Mode*)            local v; v=$(pick "Bot mode" "matchmaking" "passive") || true; [ -n "${v:-}" ] && MODE="$v" ;;
            Threads*)         local v; v=$(pick "Engine threads" 1 2 4 6 8 10 12) || true; [ -n "${v:-}" ] && THREADS="$v" ;;
            Pondering*)       local v; v=$(pick "Pondering" "on" "off") || true; [ -n "${v:-}" ] && PONDER="$v" ;;
            "Game type"*)     local v; v=$(pick "Game type" "rated" "casual") || true; [ -n "${v:-}" ] && RATED="$v" ;;
            "Rating range"*)  local v; v=$(pick "Opponent rating range (±)" 100 200 300 500 800) || true; [ -n "${v:-}" ] && RATING_DIFF="$v" ;;
            "Time controls"*)
                local v; v=$(pick_multi "Time controls to offer" \
                    "60+0" "120+1" "180+0" "180+2" "300+0" "300+3" "600+0" "600+5" "900+10") || true
                [ -n "${v:-}" ] && TCS=$(echo $v | tr '\n' ' ' | sed 's/ *$//') ;;
            "▶  START BOT")   return 0 ;;
            "✖  Quit")        return 1 ;;
            *) ;;
        esac
    done
}

fallback_settings() {
    clear; banner; echo
    echo "  1) Matchmaking   2) Passive   q) Quit"
    read -p "  Mode [1/2/q]: " c
    case "$c" in 1) MODE=matchmaking;; 2) MODE=passive;; *) exit 0;; esac
    read -p "  Threads [10]: " t; [ -n "$t" ] && THREADS="$t"
    read -p "  Pondering on/off [on]: " p; [ -n "$p" ] && PONDER="$p"
}

generate_config() {
    PONDER="$PONDER" MODE="$MODE" THREADS="$THREADS" RATED="$RATED" \
    RATING_DIFF="$RATING_DIFF" TCS="$TCS" \
    python3 - "$BASE_CONFIG" "$GEN_CONFIG" <<'PY'
import os, sys, yaml
base, out = sys.argv[1], sys.argv[2]
c = yaml.safe_load(open(base))
mode = os.environ["MODE"]
threads = int(os.environ["THREADS"])
ponder = os.environ["PONDER"] == "on"
rated = os.environ["RATED"]
rdiff = int(os.environ["RATING_DIFF"])
tcs = os.environ["TCS"].split()

c["token"] = "set_via_env"
c.setdefault("engine", {})
c["engine"]["ponder"] = ponder
c["engine"].setdefault("engine_options", {})["threads"] = threads
c["engine"].setdefault("uci_options", {})
c["engine"]["uci_options"]["Threads"] = threads
c["engine"]["uci_options"]["Ponder"] = ponder

inits = sorted({int(tc.split("+")[0]) for tc in tcs})
incs = sorted({int(tc.split("+")[1]) for tc in tcs})

mm = c.setdefault("matchmaking", {})
if mode == "matchmaking":
    mm["allow_matchmaking"] = True
    mm["challenge_variant"] = "standard"
    mm["challenge_initial_time"] = inits
    mm["challenge_increment"] = incs
    mm["opponent_rating_difference"] = rdiff
    mm["challenge_mode"] = rated
else:
    mm["allow_matchmaking"] = False
    ch = c.setdefault("challenge", {})
    ch["variants"] = ["standard"]
    ch["time_controls"] = ["bullet", "blitz", "rapid", "classical"]
    ch["modes"] = ["rated", "casual"]
    ch["bot_only"] = False
    ch["human_only"] = False

yaml.safe_dump(c, open(out, "w"), default_flow_style=False, sort_keys=False)
PY
}

load_token() {
    if [ -z "${LICHESS_BOT_TOKEN:-}" ] && [ -f "$TOKEN_FILE" ]; then
        LICHESS_BOT_TOKEN="$(tr -d '[:space:]' < "$TOKEN_FILE")"
        export LICHESS_BOT_TOKEN
    fi
    if [ -z "${LICHESS_BOT_TOKEN:-}" ]; then
        echo "No lichess token found." >&2
        echo "Put it in $TOKEN_FILE or export LICHESS_BOT_TOKEN." >&2
        exit 1
    fi
}

summary() {
    clear; banner; echo
    echo "  ${C_BOLD}Launching with:${C_RESET}"
    echo "    ${C_CYAN}mode${C_RESET}        $MODE"
    echo "    ${C_CYAN}threads${C_RESET}     $THREADS"
    echo "    ${C_CYAN}pondering${C_RESET}   $PONDER"
    if [ "$MODE" = "matchmaking" ]; then
        echo "    ${C_CYAN}game type${C_RESET}   $RATED"
        echo "    ${C_CYAN}rating ±${C_RESET}    $RATING_DIFF"
        echo "    ${C_CYAN}time ctrls${C_RESET}  $TCS"
    fi
    echo
    echo "  ${C_DIM}config: $GEN_CONFIG${C_RESET}"
    echo "  ${C_GRN}Ctrl+C to stop${C_RESET}"
    echo "  ─────────────────────────────────────────────"
}

if command -v fzf >/dev/null; then
    edit_settings || { echo "Cancelled."; exit 0; }
else
    fallback_settings
fi

load_token
generate_config
python3 "$SCRIPT_DIR/lichess_concurrency_patch.py" "$BOT_DIR" || echo "WARNING: concurrency patch not applied"
summary

cd "$BOT_DIR"
source venv/bin/activate
exec python lichess-bot.py --config "$GEN_CONFIG"
