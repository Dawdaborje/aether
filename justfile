web_dir := "aether_web"
aether := "cargo run --bin aether --"

# Run the backend dev server (default)
default: dev-serve

# Run the backend server in development mode
dev-serve:
    {{aether}} --serve --verbose -l debug

# Run the SvelteKit web app in development mode
dev-serve-web:
    cd {{web_dir}} && pnpm run dev

# Show the aether CLI help
help:
    {{aether}} --help

