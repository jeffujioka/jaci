//! `jaci cache`, run as the built binary with piped stdio and its config
//! folder and home folder in temporary folders.

mod common;

use std::fs;
use std::path::PathBuf;

use common::{Env, stderr};
use jaci::cache::{self, Cache};
use jaci::sysinfo::StaticVariable::{Cpu, Gpu, Hostname, Os};

const HELP_LINE: &str = "Run 'jaci --help' or 'jaci init --help' for more details.";

impl Env {
    fn cache_path(&self) -> PathBuf {
        self.config_folder().join("cache")
    }

    /// Reads the cache file, failing the test when it is missing or damaged.
    fn cache(&self) -> Cache {
        cache::read(&self.cache_path())
            .unwrap_or_else(|error| panic!("{error}"))
            .expect("the cache file is missing")
    }
}

/// A config whose text blocks are the `content` of each of `contents`.
fn config_with(contents: &[&str]) -> String {
    contents
        .iter()
        .map(|content| format!("[[banner.text.blocks]]\ncontent = '{content}'\n"))
        .collect()
}

/// Asserts that `jaci cache` exited 0 with no output.
fn assert_quiet_success(output: &std::process::Output) {
    assert_eq!(output.status.code(), Some(0), "{}", stderr(output));
    assert!(output.stdout.is_empty(), "{:?}", output.stdout);
    assert!(output.stderr.is_empty(), "{}", stderr(output));
}

#[test]
fn cache_holds_only_used_variables() {
    let env = Env::with_config(&config_with(&["Host: {hostname} up {uptime}"]));

    let output = env.jaci(&["cache"]);

    assert_quiet_success(&output);
    let cache = env.cache();
    assert!(cache.get(Hostname).is_some_and(|value| !value.is_empty()));
    for variable in [Os, Cpu, Gpu] {
        assert_eq!(cache.get(variable), None, "{}", variable.name());
    }
}

#[test]
fn cache_replaces_old_cache() {
    let env = Env::with_config(&config_with(&["{hostname}"]));
    fs::write(env.cache_path(), "os = \"old\"\n").unwrap();

    let output = env.jaci(&["cache"]);

    assert_quiet_success(&output);
    let cache = env.cache();
    assert!(cache.get(Hostname).is_some());
    assert_eq!(cache.get(Os), None);
}

#[test]
fn variables_in_commands_are_not_used() {
    let toml = format!(
        "{}[[banner.text.blocks]]\ncommand = 'echo {{gpu}}'\n",
        config_with(&["{cmd:echo {os}} {cpu}"])
    );
    let env = Env::with_config(&toml);

    let output = env.jaci(&["cache"]);

    assert_quiet_success(&output);
    let cache = env.cache();
    assert!(cache.get(Cpu).is_some());
    for variable in [Hostname, Os, Gpu] {
        assert_eq!(cache.get(variable), None, "{}", variable.name());
    }
}

#[test]
fn no_used_variables_writes_empty_cache() {
    let env = Env::new();

    let output = env.jaci(&["cache"]);

    assert_quiet_success(&output);
    assert_eq!(fs::read_to_string(env.cache_path()).unwrap(), "");
}

#[test]
fn broken_config_stops_cache() {
    let env = Env::with_config("[banner");

    let output = env.jaci(&["cache"]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let prefix = format!("jaci: config {}: line 1: ", env.config_path().display());
    assert!(stderr(&output).starts_with(&prefix), "{}", stderr(&output));
    assert!(!env.cache_path().exists());
}

#[test]
fn write_failure_names_cache() {
    let env = Env::with_config(&config_with(&["{hostname}"]));
    fs::create_dir(env.cache_path()).unwrap();

    let output = env.jaci(&["cache"]);

    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    let prefix = format!("jaci: cannot write cache {}: ", env.cache_path().display());
    assert!(stderr(&output).starts_with(&prefix), "{}", stderr(&output));
    assert!(
        stderr(&output).ends_with(&format!("\n{HELP_LINE}\n")),
        "{}",
        stderr(&output)
    );
}
