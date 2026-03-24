set shell := ["nu", "-c"]

alias i := install

# == Recipes for general purposes ==
# List all recipes.
list:
    just --list
# Build all Rust binaries and install frontend dependencies.
install:
    cargo build -r
    cd frontend; bun i

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
# Make an HTTP request to the server with the specified method, path and arguments.
http method path *args:
    use . *; http {{method}} (get-url "{{path}}") {{args}}
# Trigger the server to refresh its configuration.
refresh:
    just http post "/api/v1/refresh" ""
# Update the current preset of the auto-selector.
preset name:
    just http put  "/api/v1/streams/auto/config/preset" "{{name}}"

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
