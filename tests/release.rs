//! Installation: `cargo install` gives a `jaci` binary, and the release
//! workflow builds one archive per release target on a pushed `v*` tag.

use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;
use yaml_rust2::{Yaml, YamlLoader};

/// The targets a tagged release builds (D40).
const RELEASE_TARGETS: [&str; 3] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-gnu",
];
/// The release target built against an old glibc rather than the runner's.
const LINUX_TARGET: &str = "x86_64-unknown-linux-gnu";
/// The release's tag, as a workflow run step names it.
const TAG: &str = "\"$GITHUB_REF_NAME\"";

#[test]
#[ignore = "runs a full release build of jaci through cargo install"]
fn cargo_install_gives_jaci_binary() {
    let scratch = TempDir::new().unwrap();
    let root = scratch.path().join("root");
    let install = Command::new(env!("CARGO"))
        .args([
            "install",
            "--locked",
            "--path",
            env!("CARGO_MANIFEST_DIR"),
            "--root",
        ])
        .arg(&root)
        // Its own target folder, so it never waits on this test run's lock.
        .env("CARGO_TARGET_DIR", scratch.path().join("target"))
        .output()
        .unwrap();
    assert!(
        install.status.success(),
        "cargo install failed: {}",
        String::from_utf8_lossy(&install.stderr)
    );

    let version = Command::new(root.join("bin").join("jaci"))
        .arg("--version")
        .output()
        .unwrap();

    assert!(version.status.success());
    assert_eq!(
        String::from_utf8(version.stdout).unwrap(),
        format!("jaci {}\n", env!("CARGO_PKG_VERSION"))
    );
}

#[test]
fn tagged_release_builds_three_targets() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(".github/workflows/release.yml");
    let source = fs::read_to_string(&path).unwrap();
    let workflow = YamlLoader::load_from_str(&source).unwrap().remove(0);

    let trigger = &workflow["on"];
    assert_eq!(keys(trigger), ["push"], "the workflow runs on push alone");
    assert_eq!(keys(&trigger["push"]), ["tags"], "a push of tags alone");
    assert_eq!(strings(&trigger["push"]["tags"]), ["v*"]);

    let jobs = workflow["jobs"].as_hash().unwrap();
    let mut builds = jobs
        .iter()
        .filter(|(_, job)| !job["strategy"]["matrix"].is_badvalue());
    let (build_name, build) = builds.next().expect("a job with a build matrix");
    assert!(builds.next().is_none(), "one build matrix");

    let matrix = build["strategy"]["matrix"]["include"]
        .as_vec()
        .expect("a matrix of included targets");
    let mut targets: Vec<&str> = matrix
        .iter()
        .map(|entry| entry["target"].as_str().unwrap())
        .collect();
    targets.sort_unstable();
    assert_eq!(targets, RELEASE_TARGETS);

    // Drafted first, so no build failure leaves a public, partial release.
    let create = &workflow["jobs"][build["needs"].as_str().expect("the build needs a job")];
    assert!(
        commands(create, &Yaml::Null).iter().any(|run| {
            run.starts_with(&format!("gh release create {TAG} "))
                && run.split_whitespace().any(|word| word == "--draft")
        }),
        "the builds upload to a draft release"
    );
    assert!(
        jobs.values()
            .filter(|job| job["needs"] == *build_name)
            .any(|job| commands(job, &Yaml::Null)
                .contains(&format!("gh release edit {TAG} --draft=false"))),
        "a job after every build publishes the release"
    );

    for entry in matrix {
        let target = entry["target"].as_str().unwrap();
        let steps = commands(build, entry);
        let step = |wanted: &dyn Fn(&str) -> bool, what: &str| {
            steps
                .iter()
                .position(|step| wanted(step))
                .unwrap_or_else(|| panic!("{target}: {what} in {steps:#?}"))
        };

        let built = if target == LINUX_TARGET {
            // glibc 2.17, the oldest Rust supports, not the runner's own.
            let zig = step(&|step| step.starts_with("mlugg/setup-zig@"), "zig");
            let zigbuild = step(
                &|step| step == "cargo install --locked cargo-zigbuild",
                "cargo-zigbuild",
            );
            let built = step(
                &|step| step == format!("cargo zigbuild --release --locked --target {target}.2.17"),
                "a release build against glibc 2.17",
            );
            let checked = step(
                &|step| {
                    step.contains("objdump -T") && step.contains("GLIBC_") && step.contains("2.17")
                },
                "a check of the binary's glibc symbol versions",
            );
            assert!(zig < built && zigbuild < built && built < checked);
            checked
        } else {
            step(
                &|step| step == format!("cargo build --release --locked --target {target}"),
                "a release build from the lock file",
            )
        };
        if entry["native"].as_bool().unwrap() {
            let tested = step(&|step| step == "cargo test --locked", "the tests");
            assert!(tested < built, "{target}: tested before it is built");
        }

        let archive = format!("jaci-${{{{ github.ref_name }}}}-{target}.tar.gz");
        let packaged = step(
            &|step| step.starts_with(&format!("tar -czf {archive} ")),
            "the archive",
        );
        let uploaded = step(
            &|step| {
                step.starts_with(&format!("gh release upload {TAG} "))
                    && step.contains(&format!(" {archive}"))
                    && step.split_whitespace().any(|word| word == "--clobber")
            },
            "an upload of the archive that a re-run may repeat",
        );
        assert!(
            built < packaged && packaged < uploaded,
            "{target}: step order"
        );
    }
}

/// What the steps of `job` run for the matrix `entry`, in order: each
/// step's `uses` or `run`, with `entry`'s values for its `${{ matrix.* }}`,
/// leaving out the steps whose `if` the entry fails.
fn commands(job: &Yaml, entry: &Yaml) -> Vec<String> {
    job["steps"]
        .as_vec()
        .expect("steps")
        .iter()
        .filter(|step| match step["if"].as_str() {
            None => true,
            Some(condition) => match condition.strip_prefix('!') {
                Some(key) => !matrix_flag(entry, key),
                None => matrix_flag(entry, condition),
            },
        })
        .map(|step| {
            let command = step["uses"].as_str().or(step["run"].as_str()).unwrap();
            entry.as_hash().into_iter().flatten().fold(
                command.trim().to_owned(),
                |command, (key, value)| {
                    let value = match value {
                        Yaml::Boolean(value) => value.to_string(),
                        value => value.as_str().unwrap().to_owned(),
                    };
                    command.replace(
                        &format!("${{{{ matrix.{} }}}}", key.as_str().unwrap()),
                        &value,
                    )
                },
            )
        })
        .collect()
}

/// The boolean `entry` holds for the step condition `matrix.<key>`.
fn matrix_flag(entry: &Yaml, condition: &str) -> bool {
    let key = condition
        .strip_prefix("matrix.")
        .unwrap_or_else(|| panic!("a condition on a matrix flag, not {condition:?}"));
    entry[key]
        .as_bool()
        .unwrap_or_else(|| panic!("a boolean matrix.{key}"))
}

/// The keys of the mapping `yaml`, in order.
fn keys(yaml: &Yaml) -> Vec<&str> {
    yaml.as_hash()
        .expect("a mapping")
        .keys()
        .map(|key| key.as_str().unwrap())
        .collect()
}

/// The strings of the sequence `yaml`, in order.
fn strings(yaml: &Yaml) -> Vec<&str> {
    yaml.as_vec()
        .expect("a sequence")
        .iter()
        .map(|item| item.as_str().unwrap())
        .collect()
}
