<!-- markdownlint-disable MD033 MD041 MD036 -->
<p align="center">
  <img src="monke.png" width="220" alt="the one true monke">
</p>

# PNGres

> *file storage with extra steps.*
> A local-first database in which the database is monkes.

PNGres is a database. The database is monkes. The monkes are stored as PNG files in a directory on your laptop. When you upload a file, the file becomes more monkes. When you download a file, the monkes become your file again. The monkes are fine.

We have spent more than a reasonable amount of time ensuring that the monkes are fine.

Every stored byte lives in the least-significant bits of a 650×800 RGBA image of a monke. Per-file metadata lives in three private PNG ancillary chunks (`mkHd`, `mkMf`, `mkCk`) baked into every shard. There is no SQL. There is no key-value store. There is no index file. There is only `data/<file_id>/monke_001.png, monke_002.png, …`, and whatever the monkes remember.

The monkes remember a lot, actually. BLAKE3 per shard. BLAKE3 of the whole file. A mirrored copy of every header in the pixel LSBs in case something strips the ancillary chunks. A full JSON manifest duplicated into *every single shard* so that, in the event that only one monke survives the forthcoming apocalypse, the surviving monke will still know what file it used to be part of. This is, in our opinion, the responsible thing to do.

You can stream an MP4 out of a herd of monkes. You can hand a friend a zip file full of monkes and they can hand you back a movie. You can point your PNGres at another PNGres and pull that PNGres's monkes. The monkes are, in every sense that matters, federated.

---

## Quickstart

```fish
git clone https://github.com/ark-232/pngres
cd pngres
cargo install --path crates/pngres-cli   # installs `pngres` into ~/.cargo/bin

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

Launch the web UI:

```fish
just serve                       # binds 127.0.0.1:6969 by default
PNGRES_HOST=0.0.0.0 just serve   # LAN / remote friends

open http://127.0.0.1:6969
```

Drop a file. Watch monkes materialize. Click "explore herd" on the resulting file to see every shard. Click any monke to see what's hidden inside its pixels. Hit play on a video. Seek around. Open DevTools → Network tab and admire the parade of `206 Partial Content` responses.

---

## What actually happens when you upload a file

1. The file arrives on the server as a single blob of bytes. The server is briefly burdened with its entirety.
2. The server chops the blob into slices exactly `payload_bytes_per_monke` long. At the default 2 bits per channel, that's ~508 KiB per slice.
3. For each slice, the server creates a new copy of `monke.png` in memory. Every `rayon` worker gets its own copy (via `Arc<[u8]>`, so it's free).
4. The worker writes three private ancillary chunks into the copy: `mkHd` (fixed 64 B header — magic, format version, UUIDv7 file_id, chunk index, total chunks, byte offset, payload length, total file size, CRC32), `mkMf` (the full file manifest as JSON, ~300 B), and — after the `IDAT` — `mkCk` (a 32-byte BLAKE3 of the slice).
5. The worker also flips the low bits of the pixel bytes to encode the same header plus the payload plus the payload's hash. Redundant on purpose.
6. The modified PNG gets written to `data/<file_id>/monke_NNN.png`. A row is *not* inserted into a database, because there isn't one.
7. The server emits an `Upload` event onto its internal bus. Anyone subscribed to `/api/events` sees: *your IP, the filename, how many monkes, how many bytes.*

The monkes now exist. They were not there before. They are now there.

---

## What actually happens when a friend pulls a monke from you

1. Their `pngres remote pull-monkes <your-url> <file-id>` command (or their curl) hits `GET /api/files/<file-id>/monkes`. Your server emits an `IndexListed` event.
2. Your server returns a JSON list of shard URLs. No bytes of payload leave the building yet.
3. They fetch each shard in turn: `GET /api/files/<file-id>/monkes/0`, `/1`, `/2`, etc. For each one, your server emits a `ShardServed` event with *their IP, the chunk index, and the shard size*.
4. Each shard is a valid PNG. `Content-Type: image/png`. `Cache-Control: public, max-age=31536000, immutable` because the shard is content-addressed by its embedded BLAKE3 and will never change.
5. On their end, `pngres remote pull-monkes` immediately calls `decode_shard_bytes` on every byte it receives, verifies the BLAKE3, and refuses to save any shard that lies.
6. Once they have all the shards, they call `pngres decode` (or the CLI does it automatically) and reassemble the file. The final BLAKE3 is checked against the one embedded in every shard's manifest.
7. At no point do they trust your server to *interpret* bytes on their behalf. They trust it only to *move* them. If you're wondering: yes, this is the same trust model as IPFS, Bittorrent, or any content-addressed system. The system just happens to identify its content by "the least-significant bits of a PNG of a monkey."

Meanwhile on your side the live activity ticker says:

```
12:47:03 ☰  203.0.113.42 discovered movie.mp4 (412 monkes)
12:47:04 ○  203.0.113.42 pulled monke 1 of 412 of movie.mp4
12:47:04 ○  203.0.113.42 pulled monke 2 of 412 of movie.mp4
12:47:04 ○  203.0.113.42 pulled monke 3 of 412 of movie.mp4
…
```

---

## Features (exhaustive)

- **Encode any file** into a sequence of 650×800 RGBA monkes. Arbitrary format. Text, PDFs, JPEGs, MP3s, MP4s, zip files, other PNGres herds, your tax return. The monkes don't care.
- **Decode that sequence back** into the original file, byte-identical, verified via BLAKE3 (per shard and over the whole file) and independently via SHA-256 (the CLI round-trip tests).
- **Dual storage** — the payload sits in both private PNG ancillary chunks (fast metadata reads) AND in pixel LSBs (survives metadata stripping). Either path alone is sufficient to reconstruct the file.
- **Random access** — the server answers HTTP `Range` requests by computing the covering shard window via pure integer division, opening only those shards, decoding just the required byte range, and returning it as `Body::from_stream`.
- **Streaming video / audio** — `<video>` and `<audio>` tags pointed at `/api/files/:id/stream` work correctly, including seeking (a fresh `206 Partial Content` for every timeline scrub).
- **Zip export/import** — `--zip` writes a single `.zip` of monkes; `decode` auto-detects dir vs. zip input; the zip is just monkes and a manifest, nothing proprietary.
- **Tunable density** — 1, 2, or 4 LSB bits per channel, trading perceptual fidelity for monke count.
- **Streaming upload** with a 2 GiB default cap (configurable via `PNGRES_MAX_UPLOAD_BYTES`). Multipart parsing doesn't hit axum's 2 MiB default because we layered `DefaultBodyLimit::max(cfg.max_upload_bytes)` onto the upload route.
- **Soft-delete with disk cleanup** — the file becomes unreachable immediately; its monkes are hard-removed on the same call. If this seems inconsistent to you, you are correct.
- **MIME sniffing** — if your browser or curl sent `application/octet-stream` or nothing, the server looks at the first 256 bytes with `infer` and picks a better one.
- **Zero runtime dependencies** — `pngres-server` embeds the frontend (HTML + CSS + JS + monke.png) via `rust-embed`. Single binary. You can copy it to a VPS and run it. You do not need Node. You do not need sqlite. You do not need redis. You do need the monke.
- **Self-describing shards** — any single monke carries the full manifest. Lose all but one, you still know what file it *was*.
- **P2P monke pulling** — connect to another PNGres and fetch raw shards or decoded files straight over HTTP. The shard URLs are content-cacheable forever (`Cache-Control: immutable`) because every monke carries its own BLAKE3.
- **Live activity stream** — `/api/events` is a Server-Sent Events feed. Every upload, download, range-stream, P2P shard pull, discover, and delete is emitted in real time with the peer IP. The UI has a live ticker that reads like a primate Twitter timeline: *"203.0.113.42 pulled monke 3 of 412 from clip.mp4"*.
- **Monke explorer** — click "explore herd" on any file in the UI. The whole herd unfolds as a grid of thumbnails, each labelled with chunk index and payload size. Click any monke to open the **LSB amplification modal**: three side-by-side canvases showing the original cover, this shard (visually identical), and a low-bit XOR amplified 64× that reveals exactly where your data is hidden in the pixels.
- **Peer list** — `/api/peers` returns unique IPs that touched the server recently, with event counts and last-seen timestamps. Shown live in the sidebar. Audit your own jungle.
- **Extensive tests** — 63 tests across unit, property, fixture-roundtrip (text/rs/json/pdf/png/jpg/mp3/mp4/empty/1-byte), random-access, corruption fallback, CLI happy-path, P2P API, and event/peer tracking.

---

## Architecture

```
          +-----------------+
          |     browser     |
          |   (index.html)  |
          +--------+--------+
                   | HTTP + Range
                   | SSE (/api/events)
          +--------v--------+
          |  pngres-server  |  (axum 0.7, tokio)
          |  /api/upload    |
          |  /api/stream    |
          |  /api/monkes/:n |
          |  /api/events    |  <-- SSE broadcast
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

- **`crates/pngres-core`** — the encoding library. PNG chunk I/O, LSB embed/extract, manifest serialization, streaming random-access reader. No async, no HTTP, no opinions. Can be used standalone as a Rust lib.
- **`crates/pngres-server`** — `axum` HTTP server. No SQL database — `data/<file_id>/monke_*.png` is authoritative, and every shard carries the file-level manifest in its `mkMf` chunk, so the server scans the disk at startup and holds an in-memory `HashMap<Uuid, Manifest>`. A `tokio::sync::broadcast` event bus with a 500-event ring buffer powers the live SSE feed. `rust-embed` ships the frontend.
- **`crates/pngres-cli`** — `clap` CLI. Directory or zip output. Streams shards through either sink. `remote` subcommand speaks to a running server.
- **`web/dist/`** — static HTML + CSS + vanilla JS. No Node pipeline. ~900 lines of frontend. Renders the LSB visualization entirely client-side using a `<canvas>`.

---

## CLI Reference

| Command | What it does |
| --- | --- |
| `pngres encode <FILE> --out <DIR>` | Encode a file into a directory of monkes. |
| `pngres encode <FILE> --zip <PATH>` | Encode a file into a single zip of monkes. |
| `pngres encode <FILE> --bits 1\|2\|4 ...` | Choose LSB bit depth. 2 is the default. |
| `pngres decode <DIR-OR-ZIP> --out <FILE>` | Reassemble a file from a herd. |
| `pngres inspect <MONKE>` | Print everything that monke knows about itself. |
| `pngres inspect <MONKE> --json` | Same, in JSON. |
| `pngres capacity` | Print capacity-per-monke for every bit depth. |
| `pngres serve --data-dir <DIR>` | Launch `pngres-server` (requires `--features serve`, on by default). |
| `pngres remote list <URL>` | List files on a remote PNGres. |
| `pngres remote discover <URL> <ID>` | Print a remote file's monke index. |
| `pngres remote pull <URL> <ID> --out <FILE>` | Stream a decoded file from a remote (BLAKE3-verified). |
| `pngres remote pull-monkes <URL> <ID> --out <DIR>` | Pull the raw shard PNGs for local decode. |
| `pngres remote mirror <URL> --out <DIR>` | Pull *every* file from a remote. |

CLI exit codes: `0 ok` · `1 IO` · `2 clap` · `3 corruption` · `4 missing-shard` · `5 hash-mismatch` · `6 dest-exists`.

---

## HTTP API (abridged)

| Method | Path | Purpose |
| --- | --- | --- |
| POST | `/api/upload` | Multipart file upload. Returns id/name/mime/size/num_monkes/uploaded_from. |
| GET | `/api/files` | List files (newest first). |
| GET | `/api/files/:id` | Full detail incl. the complete `Manifest`. |
| GET | `/api/files/:id/download` | Full body, `Content-Disposition: attachment`. |
| GET | `/api/files/:id/stream` | Range-aware. Feeds `<video>`. 206 / 200 / 416. |
| GET | `/api/files/:id/monkes` | P2P: index of raw shard URLs. |
| GET | `/api/files/:id/monkes/:n` | P2P: the n-th raw monke PNG, immutable cache. |
| DELETE | `/api/files/:id` | Delete file + shards. |
| GET | `/api/stats` | Aggregate counters for the UI. |
| GET | `/api/events` | **Server-Sent Events** — live feed of activity. |
| GET | `/api/activity` | JSON snapshot of the recent event ring buffer. |
| GET | `/api/peers` | Unique peer IPs + event counts + first/last seen. |
| GET | `/healthz` | `"ok"`. |

---

## Performance (Apple silicon, arm64)

Measured via `cargo bench -p pngres-core --bench throughput` at 2-bit LSB:

| Operation | 1 MiB input | 10 MiB input |
| --- | --- | --- |
| Encode (8 cores, `rayon`) | ~182 MiB/s | ~473 MiB/s |
| Decode (1 core, sequential) | ~64 MiB/s | ~72 MiB/s |

Encode scales near-linearly with `rayon` because every shard is independent: clone the `Arc<[u8]>` cover, twiddle LSBs, re-encode PNG at `Compression::Fast`. Decode stays single-threaded so HTTP range consumers see strictly ordered bytes without reorder buffers; this is a deliberate latency-over-throughput choice and we would happily revisit.

One monke on disk at 2-bit LSB weighs ~700 KiB after PNG compression (the LSB churn is high-entropy and compresses poorly). Expect roughly **1.4× storage overhead** vs. the original file size. This is the price of admission to the monke economy.

---

## Testing

```fish
just test                        # 63 tests, ~10s on an M-series
just bench                       # criterion, ~1 min
just docs                        # the Typst PDF spec
```

Coverage:

| Count | Scope |
| --- | --- |
| 21 | Core unit tests (`capacity`, `chunk`, `cover`, `encode`, `hashing`, `lsb`, `manifest`) |
| 14 | Real-fixture roundtrips (txt/rs/json/pdf/png/jpg/mp3/mp4/empty/1-byte, MP4 at 1/2/4 LSB, 25 random byte-ranges into MP4) |
| 11 | Synthetic-payload roundtrips incl. boundary and cross-boundary cases |
|  3 | Random-access tests (crossing, zero-length, past-EOF, missing-shard) |
|  3 | Property tests (`proptest`) — arbitrary `Vec<u8>` roundtrips at each bit depth |
|  1 | `mkHd` strip → LSB-mirror fallback |
|  5 | CLI happy-path via `assert_cmd` incl. dir+zip roundtrip and the exit-code-6 dest-exists case |
|  3 | P2P API: full MP4 flow, multi-fixture stats, pull-then-redecode preserves bytes across 5 file types |
|  2 | Event bus / peer capture: every event kind is emitted with a peer IP; `uploaded_from` propagates |

Every test runs against real fixture files — including a 3-second H.264 MP4 that the streaming test fetches via HTTP range requests and re-verifies with `ffprobe`. We don't believe in mocks where monkes are involved.

---

## Operator's Handbook

Environment variables the server honors:

| Var | Default | Meaning |
| --- | --- | --- |
| `PNGRES_HOST` | `127.0.0.1` | Bind address. Use `0.0.0.0` to be reachable on LAN. |
| `PNGRES_PORT` | `6969` | TCP port. |
| `PNGRES_DATA_DIR` | `./herd` | Where shards live. Back this up if you care. |
| `PNGRES_MAX_UPLOAD_BYTES` | `2147483648` (2 GiB) | Upload cap. |
| `RUST_LOG` | `pngres_server=info,tower_http=info` | tracing filter. `debug` is louder; `trace` is unhinged. |

Sharing with a friend:

- **Same wifi** — `PNGRES_HOST=0.0.0.0 just serve`, share your LAN IP.
- **Different networks** — Tailscale. You both install it, you share your tailnet magic-DNS name.
- **Public URL** — `cloudflared tunnel --url http://127.0.0.1:6969` prints an ephemeral `https://*.trycloudflare.com`. Tunnel dies when the process dies.

---

## FAQ

> **Why?**
> We were asked.

> **Should I use this?**
> No.

> **Is it production ready?**
> PNGres is *test-suite* ready, which is like production ready but honest.

> **Is this a joke?**
> It is a fully functional local-first content-addressed file store that treats 650×800 RGBA PNG files of a monkey as its unit of storage. We leave the taxonomic question to the reader.

> **Will this scale to petabytes of monke?**
> Theoretically yes. You will run out of disk. You will not run out of monke.

> **Does it deduplicate?**
> No. Upload the same MP4 twice, you get two herds. An `idx_files_blake3`-style content-addressed layer is one `HashMap<[u8;32], Uuid>` away; we did not build it because it would make the project less funny.

> **Is my data safe?**
> Your data is stored as the least-significant bits of a 650×800 RGBA image of a monke. The word *safe* is doing a great deal of work in your question.

> **Is it encrypted?**
> No. LSB steganography is obfuscation, not secrecy. If confidentiality matters, encrypt the input *before* feeding it to PNGres. The monkes do not need to know.

> **Can the NSA read my monkes?**
> Yes. So can you. So can anyone with `xxd` and a grudge.

> **Does it handle video?**
> Yes. The server supports HTTP Range requests, so you can stream an MP4 directly out of a monke herd. It is the correct amount of slow.

> **Can I use a different image?**
> Architecturally yes (`CoverImage::from_png_bytes` takes any 650×800 8-bit RGBA PNG). Culturally no. This is the monke database.

> **What happens if I edit a monke in Preview and save?**
> The pixel LSBs get clobbered but the ancillary `mk*` chunks survive (we flagged them safe-to-copy). If your editor *also* strips ancillary chunks, the LSB-mirror fallback still pulls the header out of the pixels — see `crates/pngres-core/tests/chunk_strip.rs`. We decode every lost-in-the-wash scenario we can think of.

> **What happens if a monke goes missing entirely?**
> `pngres decode` exits 4 with `missing shard at chunk index 42 of 137`. Reed–Solomon or any other erasure coding would save you here. We do not have that. Back up your monkes.

> **What if two monkes disagree about who they are?**
> Each shard carries a BLAKE3 of its own payload, CRC-protected inside a fixed header, *and* a copy in the LSB mirror. Disagreement is exceptionally loud. `pngres decode` will refuse to pretend otherwise.

> **What if I point PNGres at a PNG that isn't a monke?**
> `Error: unsupported cover image: cover must be 650x800`. You must bring exactly this monke.

> **Is it federated?**
> In the sense that two independent PNGres instances can exchange monkes over HTTP without a central coordinator, yes. In the sense that it implements ActivityPub, no.

> **Is it peer-to-peer?**
> The `pngres remote` subcommand lets any PNGres (or any HTTP client) pull monkes from any other PNGres. Each peer caches nothing and trusts nothing except the BLAKE3 embedded in every shard. It is peer-to-peer the way a postal system is peer-to-peer.

> **Can I torrent monkes?**
> `pngres remote pull-monkes` walks `/api/files/:id/monkes` sequentially. A parallel swarm client would be a small amount of work and a large amount of fun.

> **How do I know the remote isn't lying to me?**
> You don't trust the remote's `/stream` (decoded) endpoint. You use `pngres remote pull-monkes` + local `pngres decode`. The local decode verifies every shard's BLAKE3 against the hash embedded in every *other* shard's manifest. A lying remote produces a crashing decode.

> **What if the cover image is copyrighted?**
> It's a monke. We have not been in touch with the monke about rights.

> **Why 650×800?**
> Because that's what `monke.png` is. If we had used a 1024×1024 monke, the capacity-per-monke would be higher, but the monke would be different, which it is not.

> **Why PNG and not JPEG?**
> JPEG is lossy. Every time you save a JPEG, the monke bleeds a little. LSB steganography survives exactly zero JPEG re-encodes. PNG is lossless and carries arbitrary ancillary chunks in its spec. The PNG spec of 1996 is the unsung hero of this project.

> **Is there a progressive encoding?**
> The PNG `Adam7` interlace mode is disabled because our encoder assumes linear scanline layout for LSB embedding. Non-interlaced only.

> **Could I swap BLAKE3 for SHA-256?**
> Yes. `pngres_core::hashing` is a 20-line module. BLAKE3 is 5–10× faster.

> **Could I swap UUIDv7 for something else?**
> Yes, but UUIDv7 is time-sortable, which makes the "newest first" list trivial.

> **Can I run this on an M1?**
> We *only* run this on an M-series. The benchmarks are from an M-series. Linux x86-64 is untested but should be fine.

> **Windows?**
> Theoretically yes. `just` runs fish commands that aren't portable. `cargo test` should be fine. We have not tried. If you try, open an issue.

> **Will you accept contributions?**
> If they are in the spirit of the monke.

> **What is the spirit of the monke?**
> See above.

---

## Known Flaws

- **In-memory upload**. The upload handler reads the full multipart field into a `Vec<u8>` before encoding. For a 2 GiB upload on an 8 GiB machine this is uncomfortable. A streaming upload path is feasible given that the shard-parallel encoder already exists.
- **In-memory peer log**. `EventBus` holds a 500-event ring buffer. Restart the server, the log starts empty. If you wanted durable audit, write the ring buffer to a file on shutdown or tee events to a `events.jsonl` on the side.
- **Single-writer**. The in-memory index uses a `tokio::sync::RwLock<HashMap>`. Concurrent uploads are serialized at the DB-insert step (trivially). This is fine at laptop scale.
- **No erasure coding**. Lose a monke, lose the file. A `reed-solomon-simd` pass could make any `k-of-n` subset sufficient; we didn't do it.
- **No dedup**. Same file, uploaded twice, two herds.
- **Boot time scales with herd count**. At startup we read the first shard of every stored file to rebuild the manifest cache. For 100 files this is instant; for 100,000 it's measurable. A persisted index sidecar would solve it.
- **LSB amplification in the UI** loads both the shard and the cover into canvases and XORs them client-side. For a 650×800 image this is fine. For larger covers it would need a worker thread.

---

## Security

PNGres performs no authentication, no authorization, no encryption, and no integrity protection against adversarial remotes beyond BLAKE3. It is a local tool that happens to speak HTTP. Run it behind an authenticated reverse proxy if you expose it to the internet and care about who talks to it. We recommend treating all monke data as public once it leaves your machine.

If you need privacy, encrypt your input before feeding it to PNGres.

If you need availability, replicate your `data/` directory somewhere.

If you need integrity, that part we have covered.

---

## Roadmap (aspirational, probably)

- **Streaming uploads** without buffering the whole body.
- **Reed–Solomon** shards so `k-of-n` is enough to recover.
- **Dedupe** by content hash. Same bytes → same monkes.
- **Cover image registry** — multiple cover images in rotation; every shard records which cover it used.
- **WebSocket P2P swarm** — parallel shard fetches across N peers, BitTorrent-style.
- **Public monke index** — opt-in "discover peers who have MP4s" gossip.
- **Monke GPU** — run the LSB embed on a shader, because at some point this becomes a reasonable engineering choice.

---

## Contributing

PRs welcome. Style: [rustfmt](https://github.com/rust-lang/rustfmt) + `clippy -- -D warnings` + the existing deadpan-bureaucrat tone in UI copy. If you propose features that are funnier than the existing ones, we will probably merge them.

---

## License

MIT. Do what you want with the monke.

---

## Credits

- To the one true monke.
- To the PNG spec authors, who, in October 1996, saw fit to allow arbitrary private chunks. They could not have foreseen this, and we do not wish to know what they think.
- To anyone who read this far.
