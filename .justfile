set shell := ["nu", "-c"]

alias i := install

# == Recipes for development experience ==
# List all recipes.
list:
    just --list
# Build all Rust binaries and install frontend dependencies.
install:
    cargo build -r
    cd frontend; bun i
bun *args:
    cd frontend; bun {{args}}
tsc *args:
    cd frontend; bunx --bun tsc --noEmit {{args}}

# == Recipes for JJ version control ==
# Move the specified bookmark to the specified revision and push all changes to GitHub.
push bookmark="dev" revision="@-":
    jj bookmark move {{bookmark}} --to={{revision}}
    jj git push --all
# Pull the latest changes from GitHub and reset the working copy to the main branch.
pull bookmark="dev":
    jj git fetch
    jj new -r {{bookmark}}@origin

# == Recipes for server RESTful APIs ==
# Make an HTTP GET request
get path *args:
    use . *; http get (get-url "{{path}}") {{args}}
# Make an HTTP PUT request with the specified data.
put path data *args:
    use . *; http put (get-url "{{path}}") "{{data}}" {{args}}
# Make an HTTP POST request with the specified data.
post path data *args:
    use . *; http post (get-url "{{path}}") "{{data}}" {{args}}
# Trigger the server to refresh its configuration.
refresh:
    just post "/api/refresh" ""
set-preset name:
    just put "/api/selector/preset" "{{name}}"
get-string:
    just get "/api/strings"
set-string key value:
    just put "/api/strings/{{key}}" "{{value}}"

# == Recipes for spawning microservices ==
# Run the main server.
server *args:
    use . *; run-server {{args}}
# Run the frontend.
app *args:
    use . *; run-app {{args}}
# Run YouTube Music.
youtube-music *args:
    use . *; run-youtube-music {{args}}
# Start the specified capture pipeline. Possible values for name are "auto" and "youtube-music".
capture name *args:
    use . *; run-capture {{name}} {{args}}
# Start the keystroke counter pipeline.
kpm:
    use . *; run-kpm
# Start the microphone status monitor (polls for Cubase window).
microphone:
    use . *; run-microphone
