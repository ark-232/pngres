//! End-to-end P2P API tests.
//!
//! Spawns the axum server in-process on an ephemeral port, uses reqwest::blocking
//! (from a dedicated thread — can't block inside a tokio context) to exercise
//! every P2P endpoint against real fixtures.

use pngres_core::{decode_shard_bytes, hash_bytes, hash_to_hex};
use pngres_server::{AppState, ServerConfig};
use reqwest::blocking::Client;
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

struct TestServer {
    base: String,
    _tmp: TempDir,
    _rt: tokio::runtime::Runtime,
    _shutdown: tokio::sync::oneshot::Sender<()>,
    _handle: std::thread::JoinHandle<()>,
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests")
        .join("fixtures")
}

fn pick_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    drop(l);
    port
}

fn spawn_server() -> TestServer {
    let tmp = tempfile::tempdir().unwrap();
    let data_dir = tmp.path().join("herd");
    let port = pick_port();
    let cfg = ServerConfig {
        host: "127.0.0.1".to_owned(),
        port,
        data_dir,
        max_upload_bytes: 50 * 1024 * 1024,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(2)
        .build()
        .unwrap();

    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    let cfg_clone = cfg.clone();
    let rt_handle = rt.handle().clone();
    let handle = std::thread::spawn(move || {
        rt_handle.block_on(async move {
            let state = AppState::init(&cfg_clone).await.expect("init state");
            let app = pngres_server::routes::router(state);
            let listener = tokio::net::TcpListener::bind((cfg_clone.host.as_str(), cfg_clone.port))
                .await
                .expect("bind");
            let shutdown = async move {
                let _ = rx.await;
            };
            axum::serve(listener, app)
                .with_graceful_shutdown(shutdown)
                .await
                .expect("serve");
        });
    });

    let base = format!("http://127.0.0.1:{}", port);
    // Wait for the server to accept connections.
    let client = Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if client.get(format!("{}/healthz", base)).send().is_ok() {
            break;
        }
        if std::time::Instant::now() >= deadline {
            panic!("server didn't come up on {}", base);
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    TestServer {
        base,
        _tmp: tmp,
        _rt: rt,
        _shutdown: tx,
        _handle: handle,
    }
}

fn upload_fixture(s: &TestServer, name: &str) -> serde_json::Value {
    let path = fixtures_dir().join(name);
    let bytes = std::fs::read(&path).unwrap();
    let client = Client::new();
    let part = reqwest::blocking::multipart::Part::bytes(bytes.clone())
        .file_name(name.to_string())
        .mime_str("application/octet-stream")
        .unwrap();
    let form = reqwest::blocking::multipart::Form::new().part("file", part);
    client
        .post(format!("{}/api/upload", s.base))
        .multipart(form)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap()
}

#[test]
fn p2p_mp4_full_flow() {
    let s = spawn_server();

    // 1. Upload the MP4
    let up = upload_fixture(&s, "clip.mp4");
    let id = up.get("id").and_then(|v| v.as_str()).unwrap().to_owned();

    // 2. List: should contain exactly 1 item
    let c = Client::new();
    let list: serde_json::Value = c
        .get(format!("{}/api/files", s.base))
        .send().unwrap().error_for_status().unwrap()
        .json().unwrap();
    assert_eq!(list["total"].as_i64().unwrap(), 1);
    assert_eq!(list["items"][0]["id"].as_str().unwrap(), id);

    // 3. Discover the monke index
    let idx: serde_json::Value = c
        .get(format!("{}/api/files/{}/monkes", s.base, id))
        .send().unwrap().error_for_status().unwrap()
        .json().unwrap();
    let num = idx["num_monkes"].as_u64().unwrap() as u32;
    assert!(num >= 1);
    assert_eq!(idx["name"].as_str().unwrap(), "clip.mp4");
    // Our MIME sniffer should have corrected application/octet-stream → video/mp4.
    assert_eq!(idx["mime"].as_str().unwrap(), "video/mp4");

    // 4. Pull each raw monke and verify it decodes + carries the right file_id
    let monkes = idx["monkes"].as_array().unwrap();
    assert_eq!(monkes.len() as u32, num);
    let original = std::fs::read(fixtures_dir().join("clip.mp4")).unwrap();
    let mut assembled = Vec::with_capacity(original.len());
    for (i, m) in monkes.iter().enumerate() {
        let url = format!("{}{}", s.base, m["url"].as_str().unwrap());
        let resp = c.get(&url).send().unwrap().error_for_status().unwrap();
        let headers = resp.headers().clone();
        let bytes = resp.bytes().unwrap();

        assert_eq!(headers.get("content-type").unwrap(), "image/png");
        assert!(headers.get("cache-control").unwrap().to_str().unwrap().contains("immutable"));

        let shard = decode_shard_bytes(&bytes).expect("decode shard");
        assert_eq!(shard.header.chunk_index, i as u32, "shard {} chunk_index", i);
        assert_eq!(shard.header.file_id.to_string(), id);
        assembled.extend_from_slice(&shard.payload);
    }
    assert_eq!(assembled.len(), original.len(), "reassembled size");
    assert_eq!(
        hash_to_hex(&hash_bytes(&assembled)),
        hash_to_hex(&hash_bytes(&original)),
        "reassembled bytes hash"
    );

    // 5. /stream without Range returns 200 + Accept-Ranges
    let resp = c
        .get(format!("{}/api/files/{}/stream", s.base, id))
        .send().unwrap().error_for_status().unwrap();
    assert_eq!(resp.headers().get("accept-ranges").unwrap(), "bytes");
    assert_eq!(resp.headers().get("content-type").unwrap(), "video/mp4");
    let body = resp.bytes().unwrap();
    assert_eq!(body.len(), original.len());

    // 6. /stream with Range returns 206 and the correct slice
    let resp = c
        .get(format!("{}/api/files/{}/stream", s.base, id))
        .header("Range", "bytes=1000-1999")
        .send().unwrap();
    assert_eq!(resp.status().as_u16(), 206);
    assert_eq!(
        resp.headers().get("content-range").unwrap(),
        &format!("bytes 1000-1999/{}", original.len())
    );
    let slice = resp.bytes().unwrap();
    assert_eq!(slice.len(), 1000);
    assert_eq!(&slice[..], &original[1000..2000]);

    // 7. Out-of-range monke -> 404
    let resp = c
        .get(format!("{}/api/files/{}/monkes/999", s.base, id))
        .send().unwrap();
    assert_eq!(resp.status().as_u16(), 404);

    // 8. Unknown file -> 404
    let resp = c
        .get(format!("{}/api/files/00000000-0000-0000-0000-000000000000/monkes", s.base))
        .send().unwrap();
    assert_eq!(resp.status().as_u16(), 404);

    // 9. Past-EOF Range -> 416
    let resp = c
        .get(format!("{}/api/files/{}/stream", s.base, id))
        .header("Range", "bytes=99999999-100000000")
        .send().unwrap();
    assert_eq!(resp.status().as_u16(), 416);

    // 10. Delete -> 204 -> list empty
    let resp = c
        .delete(format!("{}/api/files/{}", s.base, id))
        .send().unwrap();
    assert_eq!(resp.status().as_u16(), 204);
    let list: serde_json::Value = c
        .get(format!("{}/api/files", s.base))
        .send().unwrap().error_for_status().unwrap()
        .json().unwrap();
    assert_eq!(list["total"].as_i64().unwrap(), 0);
}

#[test]
fn p2p_multiple_fixtures_and_stats() {
    let s = spawn_server();
    let c = Client::new();

    for name in ["hello.txt", "tiny.pdf", "blip.mp3", "tiny.png"] {
        upload_fixture(&s, name);
    }

    let stats: serde_json::Value = c
        .get(format!("{}/api/stats", s.base))
        .send().unwrap().error_for_status().unwrap()
        .json().unwrap();
    assert_eq!(stats["total_files"].as_i64().unwrap(), 4);
    assert!(stats["total_monkes"].as_i64().unwrap() >= 4);

    let list: serde_json::Value = c
        .get(format!("{}/api/files?limit=10", s.base))
        .send().unwrap().error_for_status().unwrap()
        .json().unwrap();
    assert_eq!(list["items"].as_array().unwrap().len(), 4);

    // Each mime should be sniffed correctly
    let mimes: Vec<String> = list["items"].as_array().unwrap().iter()
        .map(|i| i["mime"].as_str().unwrap().to_owned()).collect();
    assert!(mimes.iter().any(|m| m == "text/plain" || m == "application/octet-stream"));
    assert!(mimes.iter().any(|m| m == "image/png"));
    assert!(mimes.iter().any(|m| m == "audio/mpeg"));
}

#[test]
fn p2p_pull_then_redecode_preserves_bytes() {
    // The defining P2P use case: pull raw monkes from a remote, decode locally,
    // receive original bytes. We verify every fixture round-trips this way.
    let s = spawn_server();
    let c = Client::new();

    for name in ["hello.txt", "tiny.pdf", "tiny.png", "blip.mp3", "clip.mp4"] {
        let original = std::fs::read(fixtures_dir().join(name)).unwrap();
        let up = upload_fixture(&s, name);
        let id = up["id"].as_str().unwrap().to_owned();

        // Fetch the monke index
        let idx: serde_json::Value = c
            .get(format!("{}/api/files/{}/monkes", s.base, id))
            .send().unwrap().error_for_status().unwrap().json().unwrap();

        // Pull each monke, decode, reassemble
        let mut assembled = Vec::with_capacity(original.len());
        let mut monkes_sorted: Vec<_> = idx["monkes"].as_array().unwrap().iter().cloned().collect();
        // Ensure we sort by header-reported index (the server already returns in order,
        // but re-verify via decode).
        let mut pairs: Vec<(u32, Vec<u8>)> = Vec::new();
        for m in &monkes_sorted {
            let url = format!("{}{}", s.base, m["url"].as_str().unwrap());
            let bytes = c.get(&url).send().unwrap().error_for_status().unwrap().bytes().unwrap();
            let shard = decode_shard_bytes(&bytes).unwrap();
            pairs.push((shard.header.chunk_index, shard.payload));
        }
        pairs.sort_by_key(|p| p.0);
        for (_, pl) in pairs {
            assembled.extend_from_slice(&pl);
        }
        assert_eq!(assembled.len(), original.len(), "size mismatch for {}", name);
        assert_eq!(assembled, original, "bytes mismatch for {}", name);
        let _ = monkes_sorted.drain(..);
    }
}
