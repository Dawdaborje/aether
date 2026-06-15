default: dev-serve

dev-serve:
    cargo run --bin aether -- --serve  --verbose -l debug

dev-serve-web:
    cd aether_web && pnpm run dev

help:
    cargo run --bin aether -- --help
