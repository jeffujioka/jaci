//! The blocks of the full config `jaci init` writes, made from init's
//! settings and given `{cpu}` and `{gpu}` values.

mod common;

use common::content_block;
use jaci::config::{Block, TextBlock};
use jaci::init::{self, Settings};

const SAME_CHIP: (&str, &str) = ("Apple M3 Max", "Apple M3 Max");

/// The blocks of the full config for `settings`, `{cpu}` and `{gpu}`
/// resolving to `values`.
fn full_blocks(settings: &Settings, values: (&str, &str)) -> Vec<Block> {
    init::blocks(settings, || values)
}

/// The settings of `jaci init` without `-i`, the questions at `declined`
/// answered no: 0 `{hostname}`, then `{os}`, `{cpu}`, `{gpu}`, `{ram}`,
/// `{disk}` and `{uptime}`.
fn declining(declined: &[usize]) -> Settings {
    let mut settings = Settings::default();
    for &question in declined {
        settings.shown[question] = false;
    }
    settings
}

/// A text block labelled `label` with `icon`, showing `content`.
fn row(label: &str, icon: &str, content: &str) -> Block {
    let Block::Text(block) = content_block(content) else {
        unreachable!("content_block makes text blocks");
    };
    Block::Text(TextBlock {
        label: Some(label.to_owned()),
        icon: Some(icon.to_owned()),
        ..block
    })
}

/// The labels of `blocks`' labelled blocks, in order.
fn labels(blocks: &[Block]) -> Vec<&str> {
    blocks
        .iter()
        .filter_map(|block| match block {
            Block::Text(TextBlock { label, .. }) => label.as_deref(),
            Block::Title | Block::Dots => None,
        })
        .collect()
}

#[test]
fn one_chip_block_when_cpu_and_gpu_match() {
    let blocks = full_blocks(&Settings::default(), SAME_CHIP);

    assert_eq!(
        blocks,
        [
            Block::Title,
            row("os", "\u{F108}", "{os}"),
            row("chip", "\u{F2DB}", "{cpu}"),
            row("ram", "\u{F0E4}", "{ram_meter}"),
            row("disk", "\u{F0A0}", "{disk_meter}"),
            row("up", "\u{F254}", "{uptime}"),
            row("date", "\u{F073}", "{date}"),
            row("time", "\u{F017}", "{time} · {week}"),
            Block::Dots,
        ]
    );
}

#[test]
fn cpu_and_gpu_blocks_when_they_differ() {
    let blocks = full_blocks(&Settings::default(), ("Intel Core i7", "NVIDIA RTX 4070"));

    assert_eq!(blocks[2], row("cpu", "\u{F2DB}", "{cpu}"));
    assert_eq!(blocks[3], row("gpu", "\u{F26C}", "{gpu}"));
    assert_eq!(
        labels(&blocks),
        ["os", "cpu", "gpu", "ram", "disk", "up", "date", "time"]
    );
}

#[test]
fn one_kept_variable_keeps_its_own_block() {
    let blocks = full_blocks(&declining(&[2]), SAME_CHIP);

    assert_eq!(blocks[2], row("gpu", "\u{F26C}", "{gpu}"));
    assert_eq!(
        labels(&blocks),
        ["os", "gpu", "ram", "disk", "up", "date", "time"]
    );
}

#[test]
fn declined_variables_drop_their_blocks() {
    let blocks = full_blocks(&declining(&[0, 1]), SAME_CHIP);

    assert!(!blocks.contains(&Block::Title), "{blocks:?}");
    assert_eq!(
        labels(&blocks),
        ["chip", "ram", "disk", "up", "date", "time"]
    );
    assert_eq!(blocks.last(), Some(&Block::Dots));
}
