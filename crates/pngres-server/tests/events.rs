//! Verify the event bus, peer capture, and SSE/activity/peers endpoints.

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
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(shutdown)
            .await
            .expect("serve");
        });
    });

    let base = format!("http://127.0.0.1:{}", port);
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

fn upload(s: &TestServer, name: &str) -> String {
    let bytes = std::fs::read(fixtures_dir().join(name)).unwrap();
    let client = Client::new();
    let part = reqwest::blocking::multipart::Part::bytes(bytes)
        .file_name(name.to_string())
        .mime_str("application/octet-stream")
        .unwrap();
    let form = reqwest::blocking::multipart::Form::new().part("file", part);
    let resp: serde_json::Value = client
        .post(format!("{}/api/upload", s.base))
        .multipart(form)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    resp["id"].as_str().unwrap().to_owned()
}

#[test]
fn activity_records_every_event_type_with_peer() {
    let s = spawn_server();
    let c = Client::new();

    // Upload a fixture (emits `upload`)
    let id = upload(&s, "clip.mp4");

    // List monkes (emits `index_listed`)
    c.get(format!("{}/api/files/{}/monkes", s.base, id))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();

    // Pull shard 0 (emits `shard_served`)
    c.get(format!("{}/api/files/{}/monkes/0", s.base, id))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();

    // Range-stream (emits `stream`)
    c.get(format!("{}/api/files/{}/stream", s.base, id))
        .header("Range", "bytes=100-199")
        .send()
        .unwrap();

    // Download (emits `download`)
    c.get(format!("{}/api/files/{}/download", s.base, id))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();

    // Delete (emits `delete`)
    c.delete(format!("{}/api/files/{}", s.base, id))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap();

    let activity: Vec<serde_json::Value> = c
        .get(format!("{}/api/activity", s.base))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();

    // Expect at least: index_ready, upload, index_listed, shard_served, stream, download, delete
    let kinds: Vec<String> = activity
        .iter()
        .map(|ev| ev["kind"].as_str().unwrap().to_owned())
        .collect();

    for want in [
        "index_ready",
        "upload",
        "index_listed",
        "shard_served",
        "stream",
        "download",
        "delete",
    ] {
        assert!(
            kinds.iter().any(|k| k == want),
            "missing event kind {want}; got: {:?}",
            kinds
        );
    }

    // Peer should have been captured on every non-startup event
    for ev in activity.iter() {
        if ev["kind"].as_str().unwrap() == "index_ready" {
            continue;
        }
        let peer = ev["peer"].as_str().unwrap_or("");
        assert!(
            peer == "127.0.0.1" || peer == "::1",
            "peer should be a loopback IP, got: {}",
            peer
        );
    }

    // Peers endpoint should have a single entry covering several events
    let peers: Vec<serde_json::Value> = c
        .get(format!("{}/api/peers", s.base))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    assert!(!peers.is_empty());
    let first = &peers[0];
    let events = first["events"].as_i64().unwrap();
    assert!(events >= 6, "peer event count too low: {}", events);
}

#[test]
fn upload_response_includes_uploaded_from() {
    let s = spawn_server();
    let bytes = std::fs::read(fixtures_dir().join("tiny.pdf")).unwrap();
    let client = Client::new();
    let part = reqwest::blocking::multipart::Part::bytes(bytes)
        .file_name("tiny.pdf".to_string())
        .mime_str("application/pdf")
        .unwrap();
    let form = reqwest::blocking::multipart::Form::new().part("file", part);
    let resp: serde_json::Value = client
        .post(format!("{}/api/upload", s.base))
        .multipart(form)
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    let peer = resp["uploaded_from"].as_str().unwrap_or("");
    assert!(peer == "127.0.0.1" || peer == "::1", "got: {peer}");

    // /api/files should propagate the uploaded_from
    let list: serde_json::Value = client
        .get(format!("{}/api/files", s.base))
        .send()
        .unwrap()
        .error_for_status()
        .unwrap()
        .json()
        .unwrap();
    let from = list["items"][0]["uploaded_from"].as_str().unwrap_or("");
    assert!(from == "127.0.0.1" || from == "::1");
}
