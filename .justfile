set shell := ["nu", "-c"]

alias i := install

# == Recipes for general purposes ==
# List all recipes.
list:
    just --list
# Build all Rust binaries and install all Bun dependencies.
install:
    cargo build -r
    cd frontend; bun i
    cd server; bun i

# == Recipes for JJ version control ==
# Move the specified bookmark to the specified revision and push all changes to GitHub.
push bookmark revision="@-":
    jj bookmark move {{bookmark}} --to={{revision}}
    jj git push --all
# Pull the latest changes from GitHub and reset the working copy to the main branch.
pull:
    jj git fetch
    jj git new -r main

# == Recipes for the Bun/Hono server ==
# Run the Bun/Hono server.
server *args:
    use . *; run-server {{args}}

# refresh:
#     http post $"{{base_url}}/api/v1/refresh" ""
# capture name:
#     http put $"{{base_url}}/api/v1/streams/auto/config/preset" "{{name}}"

# == Recipes for live-app ==
# Run the frontend.
app *args:
    use . *; run-app {{args}}
# Run YouTube Music.
youtube-music *args:
    use . *; run-youtube-music {{args}}

# == Recipes for live-capture and live-kpm ==
# Start the specified capture pipeline. Possible values for name are "auto" and "youtube-music".
capture name *args:
    use . *; run-capture {{name}} {{args}}
# Start the keystroke counter pipeline.
kpm:
    use . *; run-kpm
