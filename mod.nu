# Nekomaru LiveUI — Nushell orchestration module.
#
# Usage:
#   use . *
#   live-server          # start the TS relay server + Vite
#   live-capture-auto    # start auto-selector capture pipeline
#   live-capture-ytm     # start YouTube Music crop capture pipeline
#   live-kpm             # start keystroke counter pipeline

const BINARY_DIR = (
    path self
        | path dirname
        | path join "target" "release")
const SERVER_DIR = (
    path self
        | path dirname
        | path join "server")

def check-env [var: string]: nothing -> nothing {
    if ($env | get -o $var) == null {
        error make { msg: $"Environment variable ($var) is not set" }
    }
}

# Open a command in a new Windows Terminal tab with login shell.
export def spawn-tab [...args: string]: nothing -> nothing {
    wt -w 0 nt nu -l -c ($args | str join " ")
}

def --env patch-env [var: string, value: string]: nothing -> nothing {
    if ($env | get -o $var) == null {
        print $"($var) is not set. Temporarily set ($var) = \"($value)\"? [Y/n]"
        if (input) in ["Y", "y", ""] {
            load-env { $var: $value }
        } else {
            error make { msg: $"($var) is not set." }
        }
    }
}

# ── Launcher for the Bun/Hono server ───────────────────────────────────────────────────────────────────

# Start the M4 TS relay server (Bun/Hono) with Vite dev server.
# Requires LIVE_PORT and LIVE_VITE_PORT environment variables.
export def --wrapped run-server [...args]: nothing -> nothing {
    check-env "LIVE_PORT"
    check-env "LIVE_VITE_PORT"
    cd $SERVER_DIR; bun run src/index.ts ...$args
}

# ── Launcher for live-app ──────────────────────────────────────────────────────────────────────

export def --wrapped run-app [...args]: nothing -> nothing {
    patch-env "LIVE_HOST" $"localhost:($env.LIVE_PORT)"

    (run-app-internal app $"http://($env.LIVE_HOST)/"
        -x 1280 -y 720
        -t "Nekomaru LiveUI"
        ...$args)
}

export def --wrapped run-youtube-music [...args]: nothing -> nothing {
    (run-app-internal youtube-music "https://music.youtube.com/"
        -x 1280 -y 720 -s 2
        -t "YouTube Music - Nekomaru LiveUI"
        ...$args)
}

# Build live-app, copy it as live-app.<copy_id>.exe and execute the copy.
#
# Separate copy IDs allow multiple live-app processes (e.g. frontend and
# youtube-music) to run simultaneously without blocking new builds.
export def --wrapped run-app-internal [copy_id: string, ...args]: nothing -> nothing {
    let main_path = $"($BINARY_DIR)/live-app.exe";
    let copy_path = $"($BINARY_DIR)/live-app.($copy_id).exe";

    cargo build -r -p live-app;
    cp -f $main_path $copy_path;
    ^$copy_path ...$args
}

# ── Launcher for live-kpm ──────────────────────────────────────────────────────────────────────

# Start the keystroke counter pipeline.
export def run-kpm []: nothing -> nothing {
    patch-env "LIVE_HOST" $"localhost:($env.LIVE_PORT)"

    let ws_url = $"ws://($env.LIVE_HOST)/api/v1/kpm/ws/input"

    (^$"($BINARY_DIR)/live-kpm.exe"
    |^$"($BINARY_DIR)/live-ws.exe" --server $ws_url)
}

# ── Capture: Auto Selector ───────────────────────────────────────────────────

# Start the auto-selector capture pipeline.
# Polls the foreground window, matches patterns from the server config,
# hot-swaps the capture session, and relays encoded frames via WebSocket.
export def "run-capture auto" []: nothing -> nothing {
    const DEFAULT_WIDTH = 1920
    const DEFAULT_HEIGHT = 1200

    patch-env "LIVE_HOST" $"localhost:($env.LIVE_PORT)"

    let config_url = $"http://($env.LIVE_HOST)/api/v1/streams/auto/config"
    let event_url = $"http://($env.LIVE_HOST)/api/core/streamInfo/main"
    let ws_url = $"ws://($env.LIVE_HOST)/api/v1/streams/ws/main/input"

    (^$"($BINARY_DIR)/live-capture.exe"
        --mode auto
        --width $DEFAULT_WIDTH --height $DEFAULT_HEIGHT
        --stream-id main
        --config-url $config_url
        --event-url $event_url
    |^$"($BINARY_DIR)/live-ws.exe"
        --mode video
        --server $ws_url)
}

# ── Capture: YouTube Music ───────────────────────────────────────────────────
const YTM_LOG_PREFIX = $"[@youtube-music nushell]"
const YTM_TITLE = "YouTube Music - Nekomaru LiveUI"
const YTM_TITLE_BAR = 48
const YTM_BAR_HEIGHT = 112
const YTM_BOTTOM_MARGIN = 12
const YTM_RIGHT_MARGIN = 96
const YTM_FPS = 15
const YTM_POLL_INTERVAL = 5sec

# Find the YouTube Music window and return its info, or null if not found.
export def find-ytm-window []: nothing -> record<hwnd: int, width: int, height: int> {
    let arr = (
        ^$"($BINARY_DIR)/enumerate-windows.exe"
            | from json
            | where {|it| $it.title | str starts-with $YTM_TITLE })
    match ($arr | length) {
        0 => null,
        1 => {
            $arr | first
        },
        _ => {
            print $"Warning: Multiple YouTube Music windows found. Using the first one."
            $arr | first
        }
    }
}

# Compute the crop geometry for the YouTube Music playback bar.
export def ytm-crop-geometry [
    window_width: int,
    window_height: int,
]: nothing -> record<min_x: int, min_y: int, max_x: int, max_y: int> {
    let full_height = $window_height + $YTM_TITLE_BAR
    let min_y = $full_height - $YTM_BAR_HEIGHT - $YTM_BOTTOM_MARGIN
    let max_y = $full_height - $YTM_BOTTOM_MARGIN
    let max_x = $window_width - $YTM_RIGHT_MARGIN
    { min_x: 0, min_y: $min_y, max_x: $max_x, max_y: $max_y }
}

# Start the YouTube Music crop & capture service.
#
# Polls for the YTM window, launches crop capture when found, and restarts
# if the window disappears and reappears.
export def "run-capture youtube-music" []: nothing -> nothing {
    patch-env "LIVE_HOST" $"localhost:($env.LIVE_PORT)"

    let ws_url = $"ws://($env.LIVE_HOST)/api/v1/streams/ws/youtube-music/input"

    loop {
        # Poll for the YouTube Music window.
        let window = find-ytm-window
        if $window == null {
            print $"($YTM_LOG_PREFIX) Waiting for YouTube Music window..."
            sleep $YTM_POLL_INTERVAL
            continue
        }

        let crop = ytm-crop-geometry $window.width $window.height
        let hwnd = $window.hwnd
        print $"($YTM_LOG_PREFIX) found window ($window), crop=($crop)"

        # Launch the crop capture pipeline.  Blocks until the process exits
        # (e.g. window closed → capture error → live-capture exits).
        try {
            (^$"($BINARY_DIR)/live-capture.exe"
                --stream-id youtube-music
                --mode crop
                --hwnd $hwnd
                --crop-min-x $crop.min_x
                --crop-min-y $crop.min_y
                --crop-max-x $crop.max_x
                --crop-max-y $crop.max_y
                --fps $YTM_FPS
            | ^$"($BINARY_DIR)/live-ws.exe"
                --mode video
                --server $ws_url)
        } catch {
            print $"($YTM_LOG_PREFIX) capture pipeline exited, will retry..."
        }

        sleep $YTM_POLL_INTERVAL
    }
}
