//! Happy-path assert_cmd tests for the pngres CLI.

use assert_cmd::Command;
use predicates::prelude::*;
use sha2::{Digest, Sha256};
use std::fs;
use tempfile::tempdir;

fn bin() -> Command {
    Command::cargo_bin("pngres").unwrap()
}

fn sha(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    hex::encode_slice_dummy(h.finalize().as_slice())
}

// Tiny hex helper (we don't want to pull a full hex crate just for tests).
mod hex {
    pub fn encode_slice_dummy(bytes: &[u8]) -> String {
        use std::fmt::Write;
        let mut s = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let _ = write!(s, "{:02x}", b);
        }
        s
    }
}

#[test]
fn help_works() {
    bin().arg("--help").assert().success();
    bin().arg("encode").arg("--help").assert().success();
    bin().arg("decode").arg("--help").assert().success();
    bin().arg("inspect").arg("--help").assert().success();
    bin().arg("capacity").arg("--help").assert().success();
}

#[test]
fn capacity_prints_table() {
    bin()
        .arg("capacity")
        .assert()
        .success()
        .stdout(predicate::str::contains("raw capacity"))
        .stdout(predicate::str::contains("user payload"));
}

#[test]
fn encode_decode_roundtrip_dir() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.bin");
    let payload: Vec<u8> = (0..4096u32).map(|v| (v & 0xFF) as u8).collect();
    fs::write(&input, &payload).unwrap();
    let out_dir = dir.path().join("herd");
    let decoded = dir.path().join("decoded.bin");

    bin()
        .arg("encode")
        .arg(&input)
        .arg("--out")
        .arg(&out_dir)
        .assert()
        .success();

    assert!(out_dir.join("manifest.json").exists());
    let any_monke = fs::read_dir(&out_dir)
        .unwrap()
        .find_map(|e| {
            e.ok().and_then(|d| {
                let name = d.file_name();
                let s = name.to_string_lossy().into_owned();
                if s.starts_with("monke_") && s.ends_with(".png") {
                    Some(d.path())
                } else {
                    None
                }
            })
        })
        .expect("at least one monke file");

    bin()
        .arg("inspect")
        .arg(&any_monke)
        .arg("--json")
        .assert()
        .success()
        .stdout(predicate::str::contains("file_id"));

    bin()
        .arg("decode")
        .arg(&out_dir)
        .arg("--out")
        .arg(&decoded)
        .assert()
        .success();

    let back = fs::read(&decoded).unwrap();
    assert_eq!(sha(&back), sha(&payload));
}

#[test]
fn encode_decode_roundtrip_zip() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.bin");
    let payload: Vec<u8> = (0..8192u32).map(|v| ((v * 7) & 0xFF) as u8).collect();
    fs::write(&input, &payload).unwrap();
    let zip_path = dir.path().join("herd.zip");
    let decoded = dir.path().join("decoded.bin");

    bin()
        .arg("encode")
        .arg(&input)
        .arg("--zip")
        .arg(&zip_path)
        .assert()
        .success();
    assert!(zip_path.exists());

    bin()
        .arg("decode")
        .arg(&zip_path)
        .arg("--out")
        .arg(&decoded)
        .assert()
        .success();

    let back = fs::read(&decoded).unwrap();
    assert_eq!(sha(&back), sha(&payload));
}

#[test]
fn dest_exists_errors_without_force() {
    let dir = tempdir().unwrap();
    let input = dir.path().join("input.bin");
    fs::write(&input, b"hi").unwrap();
    let out = dir.path().join("herd");

    bin()
        .arg("encode")
        .arg(&input)
        .arg("--out")
        .arg(&out)
        .assert()
        .success();
    bin()
        .arg("encode")
        .arg(&input)
        .arg("--out")
        .arg(&out)
        .assert()
        .failure()
        .code(6);
    bin()
        .arg("encode")
        .arg(&input)
        .arg("--out")
        .arg(&out)
        .arg("--force")
        .assert()
        .success();
}
