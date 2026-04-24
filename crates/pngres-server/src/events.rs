//! Server-side event bus. A broadcast channel for live subscribers (SSE) plus
//! a bounded ring buffer so a reconnecting client can see recent history.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use tokio::sync::broadcast;

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    /// Server started, scanned data/, index ready.
    IndexReady {
        ts: i64,
        files: usize,
        monkes: u64,
    },
    /// A file was uploaded. `peer` is the source IP.
    Upload {
        ts: i64,
        peer: Option<String>,
        id: String,
        name: String,
        mime: String,
        size: u64,
        num_monkes: u32,
    },
    /// A full-file download was served.
    Download {
        ts: i64,
        peer: Option<String>,
        id: String,
        name: String,
        size: u64,
    },
    /// A Range request against /api/files/:id/stream was served.
    Stream {
        ts: i64,
        peer: Option<String>,
        id: String,
        name: String,
        range_start: u64,
        range_end_inclusive: u64,
        slice_len: u64,
    },
    /// A raw shard PNG was served via /api/files/:id/monkes/:n. This is the
    /// P2P "wow" event — someone out there pulled one of our monkes.
    ShardServed {
        ts: i64,
        peer: Option<String>,
        id: String,
        name: String,
        chunk_index: u32,
        total: u32,
        bytes: u64,
    },
    /// A file was soft-deleted.
    Delete {
        ts: i64,
        peer: Option<String>,
        id: String,
        name: String,
    },
    /// Someone listed the monke index for a file (the typical P2P-pull
    /// entry point — a remote is about to start fetching shards).
    IndexListed {
        ts: i64,
        peer: Option<String>,
        id: String,
        name: String,
        num_monkes: u32,
    },
}

impl Event {
    pub fn ts(&self) -> i64 {
        match self {
            Event::IndexReady { ts, .. }
            | Event::Upload { ts, .. }
            | Event::Download { ts, .. }
            | Event::Stream { ts, .. }
            | Event::ShardServed { ts, .. }
            | Event::Delete { ts, .. }
            | Event::IndexListed { ts, .. } => *ts,
        }
    }

    pub fn peer(&self) -> Option<&str> {
        match self {
            Event::Upload { peer, .. }
            | Event::Download { peer, .. }
            | Event::Stream { peer, .. }
            | Event::ShardServed { peer, .. }
            | Event::Delete { peer, .. }
            | Event::IndexListed { peer, .. } => peer.as_deref(),
            Event::IndexReady { .. } => None,
        }
    }
}

pub fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Clone)]
pub struct EventBus {
    tx: broadcast::Sender<Event>,
    history: Arc<Mutex<VecDeque<Event>>>,
    history_cap: usize,
}

impl EventBus {
    pub fn new(channel_capacity: usize, history_capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(channel_capacity);
        Self {
            tx,
            history: Arc::new(Mutex::new(VecDeque::with_capacity(history_capacity))),
            history_cap: history_capacity,
        }
    }

    pub fn emit(&self, ev: Event) {
        // Log it (tracing picks up structured output for free).
        tracing::debug!(event = ?ev, "event");
        if let Ok(mut h) = self.history.lock() {
            if h.len() == self.history_cap {
                h.pop_front();
            }
            h.push_back(ev.clone());
        }
        // Broadcast; ignore "no subscribers" errors.
        let _ = self.tx.send(ev);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.tx.subscribe()
    }

    pub fn snapshot(&self) -> Vec<Event> {
        self.history
            .lock()
            .map(|h| h.iter().cloned().collect())
            .unwrap_or_default()
    }

    /// Distinct peer IPs seen across the ring buffer.
    pub fn peers(&self) -> Vec<PeerSummary> {
        let Ok(h) = self.history.lock() else {
            return Vec::new();
        };
        use std::collections::HashMap;
        let mut map: HashMap<String, PeerSummary> = HashMap::new();
        for ev in h.iter() {
            if let Some(ip) = ev.peer() {
                let entry = map.entry(ip.to_owned()).or_insert_with(|| PeerSummary {
                    ip: ip.to_owned(),
                    first_seen: ev.ts(),
                    last_seen: ev.ts(),
                    events: 0,
                });
                entry.last_seen = ev.ts().max(entry.last_seen);
                entry.first_seen = ev.ts().min(entry.first_seen);
                entry.events += 1;
            }
        }
        let mut v: Vec<PeerSummary> = map.into_values().collect();
        v.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
        v
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct PeerSummary {
    pub ip: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub events: u64,
}
