# Nekomaru LiveUI — Nushell orchestration module.

# This module provides high-level commands to launch the various components
# of the livestreaming pipeline.

# ── Constants ──

# Directory where the compiled binaries are located. We use release binaries
# for better performance and consistency.

# ── Design Rules ──
#
# Every binary invocation goes through `get-exe`, which runs `cargo build
# --release --bin <name>` to ensure the binary is up-to-date.  Binaries that
# may run concurrently across launchers (live-capture, live-ws, live-app) use
# `get-exe --copy <id>` to copy the exe before spawning — this prevents file
# locking from blocking subsequent builds on Windows.

# ── Utility Commands ──

# Build the specified Rust binary, optionally copy it with a new name, and
# return the path to the executable. This is useful for running multiple
# instances of the same binary simultaneously without blocking new builds.
export def --wrapped get-exe [name: string, --copy: string, ...args]: nothing -> string {
    const BINARY_DIR = (
        path self
            | path dirname
            | path join "target" "release")

    cargo build --release --bin $name

    let main_path = $"($BINARY_DIR)/($name).exe"
    if $copy == null {
        $main_path
    } else {
        let copy_path = $"($BINARY_DIR)/($name).($copy).exe"
        cp -f $main_path $copy_path
        $copy_path
    }
}

export def --env get-url [path: string = "/", --ws]: nothing -> string {
    patch-env "LIVE_HOST" $"localhost:($env.LIVE_PORT)"
    let protocol = if $ws { "ws" } else { "http" }
    $"($protocol)://($env.LIVE_HOST)($path)"
}

# Checks if the specified environment variable is set, and throws an error
# if not.
export def check-env [var: string]: nothing -> nothing {
    if ($env | get -o $var) == null {
        error make { msg: $"Environment variable ($var) is not set" }
    }
}

# Checks if the specified environment variable is set, and if not, prompts
# the user to temporarily set it with the provided value for the current
# session.
export def --env patch-env [var: string, value: string]: nothing -> nothing {
    if ($env | get -o $var) == null {
        print $"($var) is not set. Temporarily set ($var) = \"($value)\"? [Y/n]"
        if (input) in ["Y", "y", ""] {
            load-env { $var: $value }
        } else {
            error make { msg: $"($var) is not set." }
        }
    }
}

# ── Launcher for live-server ──

# Start the Rust/Axum server with Vite dev server proxied.
# Requires LIVE_PORT and LIVE_VITE_PORT environment variables.
export def --wrapped run-server [...args]: nothing -> nothing {
    check-env "LIVE_PORT"
    check-env "LIVE_VITE_PORT"
    ^(get-exe "live-server") ...$args
}

# ── Launcher for live-app ──

export def --wrapped run-app [...args]: nothing -> nothing {
    (^(get-exe "live-app" --copy "app") (get-url)
        -x 1280 -y 720
        -t "Nekomaru LiveUI"
        ...$args)
}

export def --wrapped run-youtube-music [...args]: nothing -> nothing {
    (^(get-exe "live-app" --copy "youtube-music")
        "youtube-music"
        "https://music.youtube.com/"
        -x 1280 -y 720 -s 2
        -t "YouTube Music - Nekomaru LiveUI"
        ...$args)
}

# ── Launcher for live-capture and live-kpm ──

# Start the auto-selector capture pipeline.
# Polls the foreground window, matches patterns from the server config,
# hot-swaps the capture session, and relays encoded frames via WebSocket.
export def "run-capture auto" []: nothing -> nothing {
    (^(get-exe "live-capture" --copy "auto")
        --mode auto
        --width 1920 --height 1200
        --stream-id main
        --config-url (get-url "/api/selector/config")
        --event-url  (get-url "/internal/streams/main/event")
    |^(get-exe "live-ws" --copy "auto")
        --mode video
        --server     (get-url --ws "/internal/streams/main"))
}

# Start the keystroke counter pipeline.
export def run-kpm []: nothing -> nothing {
    (^(get-exe "live-kpm")
    |^(get-exe "live-ws" --copy "kpm")
        --server (get-url --ws "/internal/kpm"))
}

# ── The YouTube Music capturing pipeline ──
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
        ^(get-exe "enumerate-windows")
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
    let ws_url = get-url --ws "/internal/streams/youtube-music"

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
            (^(get-exe "live-capture" --copy "youtube-music")
                --stream-id youtube-music
                --mode crop
                --hwnd $hwnd
                --crop-min-x $crop.min_x
                --crop-min-y $crop.min_y
                --crop-max-x $crop.max_x
                --crop-max-y $crop.max_y
                --fps $YTM_FPS
            |^(get-exe "live-ws" --copy "youtube-music")
                --mode video
                --server $ws_url)
        } catch {
            print $"($YTM_LOG_PREFIX) capture pipeline exited, will retry..."
        }

        sleep $YTM_POLL_INTERVAL
    }
}
