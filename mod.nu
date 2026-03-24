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

# ── Constants ──

const YOUTUBE_MUSIC_TITLE = "YouTube Music - Nekomaru LiveUI"
const CUBASE_WINDOW_TITLE = "Cubase Pro Project - Practice-0"
const CUBASE_EXECUTABLE_PATH = 'C:\Program Files\Steinberg\Cubase 14\Cubase14.exe'
const MIC_POLL_INTERVAL = 60sec
const MIC_LOG_PREFIX = "[@microphone nushell]"

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
    # Ensure LIVE_HOST is set for URL parsing.
    get-url | ignore

    (^(get-exe "live-app" --copy "app") (get-url)
        -x 1280 -y 720
        -t "Nekomaru LiveUI"
        ...$args)
}

export def --wrapped run-youtube-music [...args]: nothing -> nothing {
    # Ensure LIVE_HOST is set for URL parsing.
    get-url | ignore

    (^(get-exe "live-app" --copy "youtube-music")
        "https://music.youtube.com/"
        -x 1280 -y 720
        -t $YOUTUBE_MUSIC_TITLE
        ...$args)
}

# ── Launcher for live-capture and live-kpm ──

# Start the auto-selector capture pipeline.
# Polls the foreground window, matches patterns from the server config,
# hot-swaps the capture session, and relays encoded frames via WebSocket.
export def "run-capture auto" []: nothing -> nothing {
    # Ensure LIVE_HOST is set for URL parsing.
    get-url | ignore

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

# Start the YouTube Music crop capture pipeline.
#
# Uses `live-capture-youtube-music` which handles window discovery, DPI-aware
# crop rect computation, and auto-restart internally.  We just pipe it to
# `live-ws` for WebSocket delivery.
export def "run-capture youtube-music" []: nothing -> nothing {
    # Ensure live-capture is built — spawned internally by live-capture-youtube-music.
    get-exe "live-capture" --copy "youtube-music" | ignore
    # Ensure LIVE_HOST is set for URL parsing.
    get-url | ignore

    (^(get-exe "live-capture-youtube-music")
        -t $YOUTUBE_MUSIC_TITLE
    |^(get-exe "live-ws" --copy "youtube-music")
        --mode video
        --server (get-url --ws "/internal/streams/youtube-music"))
}

# Start the keystroke counter.
export def run-kpm []: nothing -> nothing {
    # Ensure LIVE_HOST is set for URL parsing.
    get-url | ignore

    (^(get-exe "live-kpm")
    |^(get-exe "live-ws" --copy "kpm")
        --server (get-url --ws "/internal/kpm"))
}

# Start the microphone status monitor.
#
# Poll for the Cubase window and update the $microphone computed string.
# If the window is found (exact title + executable path match), sets "on";
# otherwise deletes the key (absence = off).
export def run-microphone []: nothing -> nothing {
    # Ensure LIVE_HOST is set for URL parsing.
    get-url | ignore

    loop {
        let found = (
            ^(get-exe "enumerate-windows")
                | from json
                | where {|it|
                    $it.title == $CUBASE_WINDOW_TITLE
                    and ($it.executable_path | str ends-with $CUBASE_EXECUTABLE_PATH) }
                | is-not-empty)

        if $found {
            http put (get-url "/internal/strings/$microphone") "on"
        } else {
            http delete (get-url "/internal/strings/$microphone")
        }
        print $"($MIC_LOG_PREFIX) microphone: (if $found { 'on' } else { 'off' })"

        sleep $MIC_POLL_INTERVAL
    }
}
