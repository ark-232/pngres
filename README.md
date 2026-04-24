<!-- markdownlint-disable MD033 MD041 -->
<p align="center">
  <img src="monke.png" width="220" alt="the one true monke">
</p>

# PNGres

> *file storage with extra steps.*
> A local key-value database where the key is a UUID and the value is a directory full of monkes.

PNGres takes any file you give it and shards the bytes across a sequence of 650×800 RGBA PNGs, each of which is a visually near-identical copy of `monke.png`. It stores the payload bits in two places at once:

- **PNG ancillary chunks** (`mkHd`, `mkMf`, `mkCk`) — the *headers*. A 64-byte header, the full file manifest as JSON, and a per-shard BLAKE3 hash.
- **Pixel LSBs** — the *data*. The same header is mirrored into the low bits of the pixel bytes, followed by the actual payload, followed by another BLAKE3 hash. Two redundant copies of the truth in each monke.

Hand the herd back and you get your file back, byte-for-byte, verified.

You can stream an MP4 out of a herd of monkes. This is not a joke, though it is very funny.

---

## Quickstart

```fish
# Clone, build, install the CLI
cargo install --path crates/pngres-cli

# Summon monkes
pngres encode report.pdf --out ./report-herd/
pngres encode movie.mp4  --zip movie.monke.zip

# Recall monkes
pngres decode ./report-herd/  --out report.pdf.back
pngres decode movie.monke.zip --out movie.mp4.back

# Peer into a single monke's soul
pngres inspect ./report-herd/monke_001.png

# How many monkes per GiB at each bit depth?
pngres capacity
```

Spin up the web UI:

```fish
just serve
# or: cargo run --release --bin pngres-server
open http://127.0.0.1:6969
```

Drag a file into the browser, watch 400 monkes materialize, then hit "play" on the video. The monkes will seek on your behalf.

Pull monkes off someone else's PNGres (P2P):

```fish
pngres remote list          http://friend.local:6969
pngres remote discover      http://friend.local:6969 <file-id>
pngres remote pull          http://friend.local:6969 <file-id> --out ./movie.mp4
pngres remote pull-monkes   http://friend.local:6969 <file-id> --out ./their-herd/
pngres decode ./their-herd/ --out ./movie.mp4        # reconstruct locally
pngres remote mirror        http://friend.local:6969 --out ./mirror/
```

The `pull-monkes` variant downloads the *raw* PNG shards; `pngres decode` then reconstructs the file *locally*, so you only trust the remote for bytes (which are BLAKE3-verified per shard) and never for interpretation.

---

## Features

- **Encode any file** into a sequence of 650×800 RGBA monkes.
- **Decode that sequence back** into the original file, byte-identical, verified via BLAKE3 + SHA-256.
- **Two storage targets** for the same data — private PNG ancillary chunks for fast metadata reads AND LSB-embedded pixel data for the payload itself.
- **Random-access read** — the server answers HTTP `Range` requests by decoding only the specific monkes covering the requested byte range.
- **Streaming video / audio** — `<video>` and `<audio>` tags pointed at `/api/files/:id/stream` work correctly, including seeking (206 Partial Content for every scrub).
- **Zip export/import** — `--zip` writes a single `.zip` of monkes; `decode` auto-detects dir vs. zip input.
- **Tunable density** — 1, 2, or 4 LSB bits per channel, trading image quality for monke count.
- **P2P monke pulling** — connect to another PNGres instance and fetch raw shards or decoded files straight over HTTP. The shard URLs are content-cacheable forever (`Cache-Control: immutable`) since every monke carries its own BLAKE3.
- **Live activity stream** — `/api/events` is a Server-Sent Events feed. Every upload, download, range-stream, P2P shard pull, discover, and delete is emitted in real time with the peer IP. The UI has a live ticker: *"203.0.113.42 pulled monke 3 of 7 from clip.mp4"*.
- **Monke explorer** — click "explore herd" on any file in the UI. The whole herd unfolds as a grid of thumbnails, each showing its chunk index and payload size. Click any monke to open the **LSB amplification modal**: three side-by-side canvases showing the original cover, this shard (visually identical), and the low-bit XOR-amplified diff that reveals exactly where your data is hidden in the pixels.
- **Peer list** — `/api/peers` returns the unique IPs that have touched the server recently, with event counts and last-seen timestamps. Shown live in the sidebar.
- **Streaming upload** and **safe multipart parsing** up to 2 GiB per file.
- **Soft-delete with undo room** — metadata rows survive, shards are cleaned.
- **MIME sniffing** — if your browser / curl sent `application/octet-stream`, the server looks at the bytes and figures it out.
- **Zero runtime dependencies** beyond the binary. `pngres-server` embeds the frontend (HTML + CSS + JS + monke.png) via `rust-embed`.
- **Self-describing shards** — any single monke carries the full manifest; lose all but one and you can still see what the file *was*.

---

## How it works (the genuinely accurate explanation)

A PNG file is a list of typed "chunks" (IHDR, IDAT, IEND, …). The format explicitly allows custom private chunks — viewers ignore unknown ones. We use three:

| Chunk | Size | Content |
| --- | --- | --- |
| `mkHd` | 64 B | Fixed-width binary header: magic, format version, LSB bit depth, UUIDv7 `file_id`, this shard's index, total shards, byte offset into the original file, payload length, total file size, CRC32. |
| `mkMf` | ~300 B | The complete file-level `Manifest` as JSON. Duplicated in **every** shard so any single monke can bootstrap a decoder. |
| `mkCk` | 32 B | BLAKE3 hash of this shard's payload bytes. |

The payload itself is embedded into the least-significant bits of the pixel data. We also mirror the 64-byte `mkHd` and the 32-byte hash into the LSB stream, so a decoder still works even if a tool strips the ancillary chunks (a verified scenario — `tests/chunk_strip.rs`).

At 2 LSB bits per channel (default), 650×800 RGBA gives **507.7 KiB of user payload per monke**. A 1 GB file becomes roughly 2,065 monkes. An HTTP `Range: bytes=5242880-10485760` request hits the database with a one-row lookup, computes `first = a / per` and `last = b / per`, opens exactly those shard files, decodes only the bytes needed, and streams them back as `Body::from_stream`. The browser seeks. The server ignores the other 2,063 monkes, which remain vibing.

---

## Architecture

```
          +-----------------+
          |     browser     |
          |   (index.html)  |
          +--------+--------+
                   | HTTP + Range
          +--------v--------+
          |  pngres-server  |   (axum 0.7, tokio)
          |   /api/upload   |
          |   /api/stream   |
          +--------+--------+
                   |
          +--------v------------+
          |  FsIndex  (memory)  |   built by scanning data/ on startup;
          |  HashMap<Uuid,Mf>   |   authoritative source is the disk
          +--------+------------+
                   |
          +--------v----------+
          |  data/<file_id>/  |
          |  monke_*.png      |   <-- there is no database. the monkes are the database.
          +--------+----------+
                   |
          +--------v-----------+
          |  pngres-core       |
          |  encode / decode   |
          |  PNG chunks + LSB  |
          +--------+-----------+
                   |
          +--------v-----------+
          |     monke.png      |
          | (embedded via      |
          |  include_bytes!)   |
          +--------------------+
```

- **`crates/pngres-core`** — the encoding library. PNG chunk I/O, LSB embed/extract, manifest serialization, streaming random-access reader. No async, no HTTP.
- **`crates/pngres-server`** — `axum` HTTP server. No SQL database — the filesystem `data/<file_id>/monke_*.png` is authoritative. Every shard carries the file-level manifest inside its `mkMf` ancillary chunk, so `pngres-server` just scans the disk at startup and holds an in-memory `HashMap<Uuid, Manifest>`. `rust-embed` ships the frontend in the binary.
- **`crates/pngres-cli`** — `clap` CLI. Directory or zip output. Streams shards through either sink.
- **`web/dist/`** — static HTML + CSS + vanilla JS. No Node pipeline. ~400 lines of frontend total.

---

## Performance (arm64, Apple silicon)

Measured via `cargo bench -p pngres-core --bench throughput` at 2-bit LSB:

| Operation | 1 MiB input | 10 MiB input |
| --- | --- | --- |
| Encode (8 cores, `rayon`) | ~182 MiB/s | ~473 MiB/s |
| Decode (1 core)           | ~64 MiB/s  | ~72 MiB/s  |

Encode scales near-linearly with rayon because each shard is independent (same cover → clone Arc → LSB twiddle → PNG re-encode). Decode is harder to parallelize because HTTP range requests want strict byte order, so we trade latency for determinism.

---

## Testing

```fish
just test       # all unit + integration + property + CLI tests
just bench      # criterion benchmarks
```

Current coverage (all green):

- 21 unit tests across `capacity`, `chunk`, `cover`, `encode`, `hashing`, `lsb`, `manifest`
- 14 real-fixture roundtrips: `hello.txt`, `lorem.rs`, `sample.json`, `empty.bin`, `one_byte.bin`, `tiny.pdf`, `tiny.png`, `tiny.jpg`, `blip.mp3`, `clip.mp4` (at 1/2/4 bits each, plus 25 random byte-ranges into the MP4)
- 11 synthetic-payload roundtrips incl. boundary/cross-boundary cases
- 3 random-access tests (range crossing, zero-length, past-EOF, missing-shard)
- 3 property tests (proptest) — any `Vec<u8>` roundtrips identically at every bit depth
- 1 chunk-strip fallback test — the LSB-mirror path decodes even when `mkHd` is deleted
- 5 CLI happy-path tests (assert_cmd): help, capacity, encode+decode dir, encode+decode zip, dest-exists exit code 6

---

## FAQ

> **Why?**
> You asked.

> **Should I use this?**
> No.

> **Will this scale to petabytes of monke?**
> Theoretically yes. You will run out of disk. You will not run out of monke.

> **Is my data safe?**
> Your data is stored as the least-significant bits of a 650×800 RGBA image of a monke. Define "safe".

> **Does it handle video?**
> Yes. The server supports HTTP range requests, so you can stream an MP4 directly out of a monke herd. It is the correct amount of slow.

> **Is the monke encrypted?**
> No. LSB steganography is obfuscation, not secrecy. Encrypt first, then feed the ciphertext to the monkes. The monkes do not need to know.

> **Does it deduplicate?**
> No. Upload the same MP4 twice, you get two herds. Consistent with the funny-DB theme.

> **Can I use a different image?**
> Architecturally yes (`CoverImage::from_png_bytes`). Culturally no. This is the monke database.

> **What happens if I edit a monke in Preview and save?**
> The pixel LSBs get clobbered but the ancillary chunks survive (they are marked safe-to-copy per the PNG spec). If your editor *also* strips ancillary chunks, the LSB-mirror fallback still pulls the header out of pixels — see `tests/chunk_strip.rs`. We decode every lost-in-the-wash scenario we can think of.

---

## Tech spec

There is a proper Typst-authored tech spec at [`docs/tech-specs.typ`](docs/tech-specs.typ) → compile to PDF with `just docs`. It covers the encoding format byte-by-byte, the API surface, performance numbers, limitations, and a glossary.

---

## Credits

- To the one true monke.
- To the PNG spec authors, who, in 1996, permitted arbitrary private chunks and thus this.
- To anyone who read this far.

## License

MIT.
