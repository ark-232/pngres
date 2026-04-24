set shell := ["fish", "-c"]
set dotenv-load := false

_default:
    @just --list

# Build everything in release mode.
build:
    cargo build --release --workspace

# Run the full test suite.
test:
    cargo test --workspace --all-features

# Lint, format-check, and test.
check:
    cargo fmt --all -- --check
    cargo clippy --workspace --all-targets --all-features -- -D warnings
    cargo test --workspace --all-features

# Run criterion benchmarks (small sizes only).
bench:
    cargo bench --workspace

# Include the 100 MiB case.
bench-big:
    cargo bench --workspace --features bench-big

# Run the HTTP server against a local herd dir.
serve dir="./herd":
    mkdir -p {{dir}}
    cargo run --release --bin pngres-server

# Run via the CLI wrapper (same thing via `pngres serve`).
serve-cli dir="./herd":
    cargo run --release --bin pngres -- serve --data-dir {{dir}}

# Compile the Typst tech spec to PDF (root set so ../monke.png resolves).
docs:
    typst compile --root . docs/tech-specs.typ docs/tech-specs.pdf

docs-watch:
    typst watch --root . docs/tech-specs.typ

# Regenerate the clip.mp4 test fixture (requires ffmpeg). Committed by default.
fixtures:
    ffmpeg -y -f lavfi -i testsrc=duration=3:size=640x480:rate=30 \
           -f lavfi -i sine=frequency=440:duration=3 \
           -c:v libx264 -preset ultrafast -pix_fmt yuv420p \
           -c:a aac -b:a 96k tests/fixtures/clip.mp4

clean:
    cargo clean
    rm -f docs/tech-specs.pdf

# CLI round-trip sanity on a small PDF.
sanity:
    cargo run --release --bin pngres -- encode tests/fixtures/tiny.pdf --out /tmp/herd-pdf --force
    cargo run --release --bin pngres -- decode /tmp/herd-pdf --out /tmp/out.pdf
    shasum -a 256 tests/fixtures/tiny.pdf /tmp/out.pdf
