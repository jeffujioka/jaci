mod common;

use std::fs;
use std::path::PathBuf;

use common::read_cache;
use jaci::cache::{self, Cache};
use jaci::named::Named;
use jaci::sysinfo::StaticVariable::{self, Cpu, Gpu, Hostname, Os, User};
use tempfile::TempDir;

/// A fresh folder (kept alive) and the path of a cache file in it.
fn cache_file() -> (TempDir, PathBuf) {
    let folder = TempDir::new().unwrap();
    let path = folder.path().join("cache");
    (folder, path)
}

fn cache_of(values: &[(StaticVariable, &str)]) -> Cache {
    values
        .iter()
        .map(|&(variable, value)| (variable, value.to_owned()))
        .collect()
}

#[test]
fn saved_values_read_back() {
    let (_folder, path) = cache_file();
    let values = [(Hostname, "h1"), (Os, "o1"), (Cpu, "c1"), (Gpu, "g1")];

    cache::write(&path, &cache_of(&values)).unwrap();
    let cache = read_cache(&path);

    for (variable, value) in values {
        assert_eq!(cache.get(variable), Some(value), "{}", variable.name());
    }
}

#[test]
fn quoted_values_read_back() {
    let (_folder, path) = cache_file();
    let value = "say \"hi\" \\ it's\nnew\tline";

    cache::write(&path, &cache_of(&[(Cpu, value)])).unwrap();

    assert_eq!(read_cache(&path).get(Cpu), Some(value));
}

#[test]
fn partial_cache_reads() {
    let (_folder, path) = cache_file();
    fs::write(&path, "hostname = \"h1\"\n").unwrap();

    let cache = read_cache(&path);

    assert_eq!(cache.get(Hostname), Some("h1"));
    for variable in [Os, Cpu, Gpu] {
        assert_eq!(cache.get(variable), None, "{}", variable.name());
    }
}

#[test]
fn missing_cache_is_none() {
    let (_folder, path) = cache_file();

    assert_eq!(cache::read(&path).unwrap(), None);
}

#[test]
fn damaged_cache_reported() {
    let (_folder, path) = cache_file();
    fs::write(&path, "hostname = ").unwrap();

    let error = cache::read(&path).unwrap_err();

    assert_eq!(
        error.to_string(),
        format!(
            "jaci: cache {} is damaged; run 'jaci cache' to rebuild it",
            path.display()
        )
    );
}

#[test]
fn other_values_and_keys_are_damaged() {
    let inputs: [&[u8]; 4] = [
        b"hostname = 1\n",
        b"[hostname]\nos = \"o1\"\n",
        b"ram = \"1 GiB\"\n",
        b"hostname = \"\xff\"\n",
    ];
    for input in inputs {
        let (_folder, path) = cache_file();
        fs::write(&path, input).unwrap();

        let error = cache::read(&path).unwrap_err().to_string();

        assert!(
            error.ends_with(" is damaged; run 'jaci cache' to rebuild it"),
            "{error}"
        );
    }
}

#[test]
fn saved_user_reads_back() {
    let (_folder, path) = cache_file();

    cache::write(&path, &cache_of(&[(User, "u1"), (Hostname, "h1")])).unwrap();
    let cache = read_cache(&path);

    assert_eq!(cache.get(User), Some("u1"));
    assert_eq!(cache.get(Hostname), Some("h1"));
}
