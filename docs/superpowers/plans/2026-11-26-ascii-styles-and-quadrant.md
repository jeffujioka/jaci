# Ascii Styles and Quadrant Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make ascii jaci's default render mode with four selectable styles (`plain`, `bright`, `dense`, `tinted`), and add an opt-in `quadrant` mode that doubles halfblock's horizontal detail at the same art size.

**Architecture:** `src/render.rs` keeps the `Mode` type, halfblock, braille and the shared cell helpers; the ascii styles move with `Charset` into `src/render/ascii.rs`, and the quadrant renderer goes into `src/render/quadrant.rs`. `size::fit` sharpens a resampled, fully opaque image in quadrant mode. The config gains `encode.ascii_style` and makes `encode.charset` optional, the command line gains `--ascii-style`, and `jaci init` writes and asks for the style.

**Tech Stack:** Rust 1.87 (edition 2024), `image` 0.25, `clap` 4 derive, `toml` 1.x; Python 3 standard library for the golden-file generator only.

**Spec:** `docs/superpowers/specs/2026-11-26-ascii-styles-and-quadrant-design.md`

## Global Constraints

- No new dependency, and `rust-version = "1.87"` stays.
- Output is identical on macOS and Linux: the brightness maths uses integers and `+ − × ÷` and `sqrt` on `f64`, in the order the spec writes them; never `powf`.
- Halfblock and braille output, and `plain` ascii, stay byte for byte what they are: `test_fixtures/4x4-halfblock.ansi`, `4x4-braille.ansi` and `4x4-ascii.ansi` are not regenerated.
- A transparent pixel (alpha 0) is a space with no escape; every coloured cell ends in `ESC[0m`; pixels past the image's edge count as transparent.
- Art width, height and proportions do not change for any existing mode.
- Mode order everywhere: `ascii, halfblock, quadrant, braille`. Style order: `plain, bright, dense, tinted`.
- Names come from the `Named` trait (`src/named.rs`): no mode or style name is written a second time in production code.
- Errors keep the crate's convention: a hand-written enum whose `Display` is the full `jaci: …` message.
- Work on a branch `feat/ascii-styles` made from `feat/jaci`. Never push.
- Gate before every commit: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps`. Every commit passes it.
- Commits: Conventional Commits, title at most 72 characters, a one- or two-bullet body.

## Review Focus

1. **A flat image in `bright`** (every opaque pixel the same luminance, so `hi = lo`): no division by zero; the level is `Y ÷ 255 000`. Pinned by `bright_flat_image_uses_plain_luminance` (Task 1).
2. **An image with no opaque pixel in `bright`, `dense` or `tinted`**: all spaces, no panic on the empty percentile list. Pinned by `bright_all_transparent_is_spaces` (Task 1).
3. **A charset given together with `dense`** (`--charset`, or `[encode].charset`): the given characters win over the style's 70. Pinned by `charset_beats_the_dense_style` (Task 4).
4. **`--ascii-style` with another mode**: ignored; halfblock output equals its golden file. Pinned by `ascii_style_is_ignored_outside_ascii` (Task 4).
5. **A quadrant image whose width or height is odd**: the missing quarters are transparent, not a panic or a wrapped index. Pinned by `quadrant_missing_pixels_are_transparent` (Task 2).

---

### Task 1: Ascii styles in the renderer

**Files:**
- Create: `src/render/ascii.rs`
- Create: `test_fixtures/make_goldens.py`, `test_fixtures/4x4-ascii-bright.ansi`, `test_fixtures/4x4-ascii-dense.ansi`, `test_fixtures/4x4-ascii-tinted.ansi`, `test_fixtures/4x4-quadrant.ansi`
- Modify: `src/render.rs` (move `DEFAULT_CHARSET`, `Charset` and `ascii` out; declare and re-export the module)
- Modify: `src/cli.rs` (`write_art`'s one call of `render::ascii`)
- Test: `tests/render.rs`

**Interfaces:**
- Consumes: `render_cells`, `opaque`, `push_cell`, `Rgb` in `src/render.rs` (private items of the parent module, reachable from `render::ascii` as `super::…`); `ansi::push_foreground`, `ansi::push_background`, `ansi::RESET`; `named::Named`.
- Produces, all re-exported from `jaci::render`:
  - `pub enum AsciiStyle { Plain, Bright, Dense, Tinted }`, deriving `Debug, Clone, Copy, PartialEq, Eq, Default` with `#[default]` on `Bright`; `impl Named` with `ALL` in that order and the names `plain`, `bright`, `dense`, `tinted`.
  - `impl AsciiStyle { pub fn charset(self) -> Charset }`: the style's own characters.
  - `pub fn ascii(image: &RgbaImage, style: AsciiStyle, charset: &Charset) -> Vec<String>`.
  - `Charset` and `pub(crate) const DEFAULT_CHARSET: &str`, unchanged, now defined in `src/render/ascii.rs`.

- [ ] **Step 1: Create the branch and commit the design and this plan on it**

```bash
git switch -c feat/ascii-styles feat/jaci
git add docs/superpowers/specs/2026-11-26-ascii-styles-and-quadrant-design.md docs/superpowers/plans/2026-11-26-ascii-styles-and-quadrant.md
git commit -m "docs: add the ascii styles and quadrant design and plan" -m "- Record the design Jeff chose and the plan that builds it"
```

Stage only those two files: `docs/superpowers/plans/2026-04-22-boto-implementation.md` is an old, replaced plan and stays untracked.

- [ ] **Step 2: Add the golden generator and run it**

Create `test_fixtures/make_goldens.py` with exactly this content. It is the spec's maths computed from the fixture's pixel table, with no jaci code involved, so the golden files cannot inherit a renderer bug.

```python
#!/usr/bin/env python3
"""Computes the golden art files of the ascii styles and of the quadrant mode
for test_fixtures/4x4.png from its pixel table, independently of jaci's
renderer. Run from the repository root: python3 test_fixtures/make_goldens.py
"""
import math
import pathlib

ESC = "\x1b"
RESET = ESC + "[0m"
# test_fixtures/4x4.png, row by row: (R, G, B, A).
PIXELS = [
    [(255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 0, 0), (10, 20, 30, 0)],
    [(0, 0, 255, 255), (0, 0, 0, 0), (255, 255, 0, 255), (10, 20, 30, 0)],
    [(255, 255, 255, 255), (128, 128, 128, 128), (0, 255, 255, 255), (64, 32, 16, 255)],
    [(0, 0, 0, 255), (200, 100, 50, 1), (0, 0, 0, 0), (17, 34, 51, 255)],
]
SHORT = " .:-=+*#%@"
DENSE = " .'`^\",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$"
QUADRANTS = " ▘▝▀▖▌▞▛▗▚▐▜▄▙▟█"


def half_away(x):
    """Rounds a number that is not negative half away from zero."""
    return int(math.floor(x + 0.5))


def fg(colour):
    return f"{ESC}[38;2;{colour[0]};{colour[1]};{colour[2]}m"


def bg(colour):
    return f"{ESC}[48;2;{colour[0]};{colour[1]};{colour[2]}m"


def luma(pixel):
    return 299 * pixel[0] + 587 * pixel[1] + 114 * pixel[2]


def bright(charset, tinted):
    ys = sorted(luma(p) for row in PIXELS for p in row if p[3] != 0)
    lo, hi = ys[2 * len(ys) // 100], ys[98 * len(ys) // 100]
    rows = []
    for row in PIXELS:
        out = ""
        for p in row:
            if p[3] == 0:
                out += " "
                continue
            y = luma(p)
            t = y / 255000 if hi == lo else min(max((y - lo) / (hi - lo), 0.0), 1.0)
            level = math.sqrt(math.sqrt(t * t * t))
            glyph = charset[min(int(level * len(charset)), len(charset) - 1)]
            value = 0.6 + 0.4 * level
            top = max(p[:3])
            if top == 0:
                colour = [half_away(value * 255)] * 3
            else:
                colour = [half_away(c / top * value * 255) for c in p[:3]]
            tint = bg([half_away(c * 0.4) for c in p[:3]]) if tinted else ""
            out += fg(colour) + tint + glyph + RESET
        rows.append(out)
    return rows


def mean(pixels):
    """The mean colour, each channel rounded half up."""
    n = len(pixels)
    return [(2 * sum(p[i] for p in pixels) + n) // (2 * n) for i in range(3)]


def quadrant():
    rows = []
    for y in range(0, 4, 2):
        out = ""
        for x in range(0, 4, 2):
            cell = [PIXELS[y][x], PIXELS[y][x + 1], PIXELS[y + 1][x], PIXELS[y + 1][x + 1]]
            opaque = [i for i, p in enumerate(cell) if p[3] != 0]
            if not opaque:
                out += " "
            elif len(opaque) < 4:
                mask = sum(1 << i for i in opaque)
                out += fg(mean([cell[i] for i in opaque])) + QUADRANTS[mask] + RESET
            else:
                best = None
                for mask in range(1, 8):
                    groups = (
                        [cell[i] for i in range(4) if mask >> i & 1],
                        [cell[i] for i in range(4) if not mask >> i & 1],
                    )
                    # 6 × the squared error, an integer: for a group of n
                    # pixels, 6·Σ|p|² − (6 ÷ n)·|Σp|².
                    score = sum(
                        6 * sum(c * c for p in group for c in p[:3])
                        - 6 // len(group) * sum(sum(p[i] for p in group) ** 2 for i in range(3))
                        for group in groups
                    )
                    if best is None or score < best[0]:
                        best = (score, mask, groups)
                _, mask, (front, back) = best
                out += fg(mean(front)) + bg(mean(back)) + QUADRANTS[mask] + RESET
        rows.append(out)
    return rows


def write(name, rows):
    """Writes an art file: each row, a reset and a newline."""
    text = "".join(row + RESET + "\n" for row in rows)
    pathlib.Path("test_fixtures", name).write_text(text, encoding="utf-8")


write("4x4-ascii-bright.ansi", bright(SHORT, tinted=False))
write("4x4-ascii-dense.ansi", bright(DENSE, tinted=False))
write("4x4-ascii-tinted.ansi", bright(SHORT, tinted=True))
write("4x4-quadrant.ansi", quadrant())
```

Run: `python3 test_fixtures/make_goldens.py && git status --short test_fixtures`
Expected: four new files, `4x4-ascii-bright.ansi`, `4x4-ascii-dense.ansi`, `4x4-ascii-tinted.ansi`, `4x4-quadrant.ansi`; the three existing golden files unchanged. The first line of `4x4-ascii-bright.ansi`, with `ESC` for the escape byte, is `ESC[38;2;194;0;0m=ESC[0mESC[38;2;0;221;0m*ESC[0m  ESC[0m`.

- [ ] **Step 3: Write the failing tests**

In `tests/render.rs`: import `AsciiStyle` and `jaci::named::Named`; give every existing `render::ascii(…)` call the argument `AsciiStyle::Plain` (their expected values do not change); add:

```rust
/// One opaque pixel per colour, in a row.
fn row(colours: &[[u8; 3]]) -> RgbaImage {
    let pixels: Vec<_> = colours.iter().map(|&[r, g, b]| Rgba([r, g, b, 255])).collect();
    image(colours.len() as u32, 1, &pixels)
}

#[test]
fn ascii_styles_have_their_names_and_bright_is_the_default() {
    assert_eq!(AsciiStyle::names(), "plain, bright, dense, tinted");
    assert_eq!(AsciiStyle::default(), AsciiStyle::Bright);
}

#[test]
fn bright_stretches_contrast_and_lifts_brightness() {
    // lo = 0 and hi = 255 000, so the levels are 0 and 1.
    assert_eq!(
        render::ascii(&row(&[[0, 0, 0], [255, 255, 255]]), AsciiStyle::Bright, &Charset::default()),
        ["\x1b[38;2;153;153;153m \x1b[0m\x1b[38;2;255;255;255m@\x1b[0m"]
    );
}

#[test]
fn bright_keeps_the_hue() {
    let image = row(&[[120, 60, 0], [240, 120, 0], [0, 0, 0], [255, 255, 255]]);

    let rows = render::ascii(&image, AsciiStyle::Bright, &Charset::default());

    assert!(
        rows[0].starts_with("\x1b[38;2;192;96;0m-\x1b[0m\x1b[38;2;219;109;0m*\x1b[0m"),
        "{rows:?}"
    );
}

#[test]
fn bright_takes_the_2nd_and_98th_percentile() {
    // 100 greys, 0 to 99: lo is the grey at index 2, hi the one at index 98.
    let greys: Vec<[u8; 3]> = (0..100).map(|grey| [grey; 3]).collect();

    let rows = render::ascii(&row(&greys), AsciiStyle::Bright, &Charset::default());

    let cells: Vec<&str> = rows[0].split_inclusive("\x1b[0m").collect();
    assert_eq!(cells.len(), 100);
    for cell in &cells[..3] {
        assert_eq!(*cell, "\x1b[38;2;153;153;153m \x1b[0m");
    }
    assert_ne!(cells[3], cells[2]);
    assert_ne!(cells[97], cells[98]);
    for cell in &cells[98..] {
        assert_eq!(*cell, "\x1b[38;2;255;255;255m@\x1b[0m");
    }
}

#[test]
fn bright_flat_image_uses_plain_luminance() {
    // hi = lo, so the level is Y ÷ 255 000 with no stretch.
    assert_eq!(
        render::ascii(&row(&[[200, 10, 10], [200, 10, 10]]), AsciiStyle::Bright, &Charset::default()),
        ["\x1b[38;2;190;10;10m-\x1b[0m\x1b[38;2;190;10;10m-\x1b[0m"]
    );
    assert_eq!(
        render::ascii(&row(&[[128, 128, 128]]), AsciiStyle::Bright, &Charset::default()),
        ["\x1b[38;2;214;214;214m+\x1b[0m"]
    );
}

#[test]
fn bright_all_transparent_is_spaces() {
    let image = image(2, 1, &[TRANSPARENT, TRANSPARENT]);

    for style in [AsciiStyle::Bright, AsciiStyle::Dense, AsciiStyle::Tinted] {
        assert_eq!(render::ascii(&image, style, &Charset::default()), ["  "]);
    }
}

#[test]
fn bright_skips_transparent_pixels_in_the_percentiles() {
    let image = image(3, 1, &[TRANSPARENT, Rgba([255, 255, 255, 255]), Rgba([0, 0, 0, 255])]);

    assert_eq!(
        render::ascii(&image, AsciiStyle::Bright, &Charset::default()),
        [" \x1b[38;2;255;255;255m@\x1b[0m\x1b[38;2;153;153;153m \x1b[0m"]
    );
}

#[test]
fn bright_with_one_character_draws_it_everywhere() {
    let rows = render::ascii(
        &row(&[[0, 0, 0], [255, 255, 255]]),
        AsciiStyle::Bright,
        &Charset::new("#").unwrap(),
    );

    assert_eq!(rows, ["\x1b[38;2;153;153;153m#\x1b[0m\x1b[38;2;255;255;255m#\x1b[0m"]);
}

#[test]
fn dense_has_70_characters_of_its_own() {
    let style = AsciiStyle::Dense;

    assert_eq!(
        render::ascii(&row(&[[0, 0, 0], [255, 255, 255]]), style, &style.charset()),
        ["\x1b[38;2;153;153;153m \x1b[0m\x1b[38;2;255;255;255m$\x1b[0m"]
    );
    for style in [AsciiStyle::Plain, AsciiStyle::Bright, AsciiStyle::Tinted] {
        assert_eq!(style.charset(), Charset::default());
    }
}

#[test]
fn tinted_adds_the_pixel_colour_at_40_percent_behind() {
    assert_eq!(
        render::ascii(&row(&[[0, 0, 0], [255, 255, 255]]), AsciiStyle::Tinted, &Charset::default()),
        [concat!(
            "\x1b[38;2;153;153;153m\x1b[48;2;0;0;0m \x1b[0m",
            "\x1b[38;2;255;255;255m\x1b[48;2;102;102;102m@\x1b[0m"
        )]
    );
}

#[test]
fn ascii_style_goldens() {
    for (style, golden) in [
        (AsciiStyle::Bright, "4x4-ascii-bright.ansi"),
        (AsciiStyle::Dense, "4x4-ascii-dense.ansi"),
        (AsciiStyle::Tinted, "4x4-ascii-tinted.ansi"),
    ] {
        assert_golden(&render::ascii(&fixture(), style, &style.charset()), golden, 4, 4);
    }
}
```

- [ ] **Step 4: Run the tests to see them fail**

Run: `cargo test --test render`
Expected: FAIL to compile: `AsciiStyle` is not found in `jaci::render`.

- [ ] **Step 5: Implement the styles in `src/render/ascii.rs`**

Move `DEFAULT_CHARSET`, `Charset` (with `new`, `pick`, `Default`) and `ascii` from `src/render.rs` into the new file; in `src/render.rs` add `mod ascii;`, `pub use ascii::{AsciiStyle, Charset, ascii};` and `pub(crate) use ascii::DEFAULT_CHARSET;`.

- `AsciiStyle::charset`: `Dense` gives the 70 characters below; every other style gives `Charset::default()`.

```rust
const DENSE_CHARSET: &str =
    " .'`^\",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";
```

- `ascii` with `AsciiStyle::Plain` does exactly what `ascii` does today (`Charset::pick`, the pixel's own colour).
- For the other three styles `ascii` first collects `Y = 299R + 587G + 114B` (a `u32`) of every opaque pixel, sorts them, and takes `lo = sorted[2k / 100]`, `hi = sorted[98k / 100]` for `k` values (integer division); with no opaque pixel it skips this and every cell is a space. Then per opaque pixel, in `f64`:
  - `t = if hi == lo { Y / 255_000.0 } else { ((Y − lo) / (hi − lo)).clamp(0.0, 1.0) }`
  - `level = (t * t * t).sqrt().sqrt()`
  - character index `((level * n as f64) as usize).min(n − 1)`
  - `value = 0.6 + 0.4 * level`
  - foreground channel `(c / max * value * 255.0).round()` with `max` the largest of R, G, B; when `max` is 0, `(value * 255.0).round()` for all three
  - `Tinted` only: background channel `(c * 0.4).round()`, written with `ansi::push_background` after the foreground and before the character.
- Keep `Charset::pick` for `Plain` only; give `Charset` a private way to index by level for the other styles.

In `src/cli.rs`, `write_art` calls `render::ascii(&fit.image, AsciiStyle::Plain, charset)` for now, so the binary's output does not change in this task; Task 4 wires the real style.

- [ ] **Step 6: Run the tests to see them pass**

Run: `cargo test --test render`
Expected: PASS, the existing tests and the eleven new ones.

- [ ] **Step 7: Run the gate and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
git add src/render.rs src/render/ascii.rs src/cli.rs tests/render.rs test_fixtures/make_goldens.py test_fixtures/4x4-ascii-bright.ansi test_fixtures/4x4-ascii-dense.ansi test_fixtures/4x4-ascii-tinted.ansi test_fixtures/4x4-quadrant.ansi
git commit -m "feat(render): add the bright, dense and tinted ascii styles" -m "- Stretch contrast between the 2nd and 98th percentile and lift brightness
- Compute the styles' golden files outside the renderer"
```

---

### Task 2: Quadrant render mode

**Files:**
- Create: `src/render/quadrant.rs`
- Modify: `src/render.rs` (`Mode::Quadrant`, the order of `Mode::ALL`, `cell_size`, the module and its re-export, the `every_mode_has_its_name` unit test)
- Modify: `src/cli.rs` (`write_art`'s `match mode`)
- Modify: `tests/cli_init.rs` (the mode prompt string in `interactive_init_asks_for_image`)
- Test: `tests/render.rs`

**Interfaces:**
- Consumes: `render_cells`, `opaque`, `push_cell`, `Rgb` from `src/render.rs`; `ansi::push_foreground`, `ansi::push_background`, `ansi::RESET`; `test_fixtures/4x4-quadrant.ansi` from Task 1.
- Produces:
  - `Mode::Quadrant`, with `cell_size()` `(2, 2)` and the name `quadrant`; `Mode::ALL` is `[Ascii, Halfblock, Quadrant, Braille]`.
  - `pub fn quadrant(image: &RgbaImage) -> Vec<String>`, re-exported as `jaci::render::quadrant`.

- [ ] **Step 1: Write the failing tests**

In `tests/render.rs` add:

```rust
#[test]
fn quadrant_transparent_cell_is_space() {
    assert_eq!(render::quadrant(&image(2, 2, &[TRANSPARENT; 4])), [" "]);
}

#[test]
fn quadrant_partial_cell_draws_its_opaque_quarters() {
    let red = Rgba([255, 0, 0, 255]);
    let blue = Rgba([0, 0, 255, 255]);

    assert_eq!(
        render::quadrant(&image(2, 2, &[red, TRANSPARENT, TRANSPARENT, TRANSPARENT])),
        ["\x1b[38;2;255;0;0m▘\x1b[0m"]
    );
    assert_eq!(
        render::quadrant(&image(2, 2, &[TRANSPARENT, blue, TRANSPARENT, blue])),
        ["\x1b[38;2;0;0;255m▐\x1b[0m"]
    );
}

#[test]
fn quadrant_partial_cell_mean_rounds_half_up() {
    let image = image(2, 2, &[Rgba([0, 0, 0, 255]), Rgba([1, 1, 1, 255]), TRANSPARENT, TRANSPARENT]);

    assert_eq!(render::quadrant(&image), ["\x1b[38;2;1;1;1m▀\x1b[0m"]);
}

#[test]
fn quadrant_full_cell_splits_into_two_colours() {
    let red = Rgba([255, 0, 0, 255]);
    let blue = Rgba([0, 0, 255, 255]);

    // Left column red, right column blue: the foreground is quarters 1 + 4.
    assert_eq!(
        render::quadrant(&image(2, 2, &[red, blue, red, blue])),
        ["\x1b[38;2;255;0;0m\x1b[48;2;0;0;255m▌\x1b[0m"]
    );
}

#[test]
fn quadrant_uniform_cell_takes_the_lowest_split() {
    let grey = Rgba([10, 20, 30, 255]);

    assert_eq!(
        render::quadrant(&image(2, 2, &[grey; 4])),
        ["\x1b[38;2;10;20;30m\x1b[48;2;10;20;30m▘\x1b[0m"]
    );
}

#[test]
fn quadrant_missing_pixels_are_transparent() {
    let red = Rgba([255, 0, 0, 255]);

    assert_eq!(
        render::quadrant(&image(3, 1, &[red; 3])),
        ["\x1b[38;2;255;0;0m▀\x1b[0m\x1b[38;2;255;0;0m▘\x1b[0m"]
    );
}

#[test]
fn quadrant_golden() {
    assert_golden(&render::quadrant(&fixture()), "4x4-quadrant.ansi", 2, 2);
}
```

In `src/render.rs`'s unit tests change `every_mode_has_its_name` to expect `"ascii, halfblock, quadrant, braille"` and add `assert_eq!(Mode::Quadrant.cell_size(), (2, 2));`.

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test --test render --lib`
Expected: FAIL to compile: no `quadrant` in `jaci::render`, no `Mode::Quadrant`.

- [ ] **Step 3: Implement `quadrant` in `src/render/quadrant.rs` and the mode**

- Quarter bits: upper-left 1, upper-right 2, lower-left 4, lower-right 8. The characters by that sum, 0 to 15:

```rust
const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];
```

- No opaque quarter: a space. One to three opaque: `push_cell` with their mean colour and the character of their bits. Four opaque: try the foreground sums 1 to 7 and keep the first with the lowest score, where for each of the two groups, of `n` pixels,

  `score += 6 × Σ(c²) − (6 ÷ n) × Σ over the three channels of (Σ c)²`

  in integers (`u32` is wide enough: at most 6 × 4 × 3 × 255²); then write the foreground mean, the background mean, the character and a reset.
- A mean rounds half up per channel, `(2 × sum + n) ÷ 2n`. Braille in `src/render.rs` already holds this rule inline: move it into one private function in `src/render.rs` that both use.
- In `src/cli.rs`, `write_art`'s match gains `Mode::Quadrant => render::quadrant(&fit.image)`.
- In `tests/cli_init.rs`, `interactive_init_asks_for_image` expects `"Render mode (ascii, halfblock, quadrant, braille) [halfblock]: "` (the default changes in Task 4).

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test --test render --test cli_init --lib`
Expected: PASS.

- [ ] **Step 5: Run the gate and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
git add src/render.rs src/render/quadrant.rs src/cli.rs tests/render.rs tests/cli_init.rs
git commit -m "feat(render): add the quadrant render mode" -m "- Draw each cell from 2×2 pixels in the two colours that fit them best
- List the modes as ascii, halfblock, quadrant, braille"
```

---

### Task 3: Sharpen resampled opaque images in quadrant mode

**Files:**
- Modify: `src/size.rs` (`fit`, two constants, one private function)
- Test: `tests/size.rs`

**Interfaces:**
- Consumes: `Mode::Quadrant` from Task 2; `size::fit(&RgbaImage, Mode, &Sizing) -> Result<Fit<'_>, TooLarge>`; `image::imageops::unsharpen`.
- Produces: no new public item. `fit` in `Mode::Quadrant` returns the resampled image sharpened when every pixel of it has alpha 255.

- [ ] **Step 1: Write the failing tests**

In `tests/size.rs` (add `use std::borrow::Cow;`):

```rust
/// A 64×64 image, its left half `left` and its right half `right`.
fn halves(left: Rgba<u8>, right: Rgba<u8>) -> RgbaImage {
    RgbaImage::from_fn(64, 64, |x, _| if x < 32 { left } else { right })
}

/// `image` resampled to 32×16 pixels as art of 16 quadrant columns, and the
/// same grid through halfblock (32 columns of 1×2 cells at ratio 0.25),
/// which never sharpens.
fn quadrant_and_plain(image: &RgbaImage) -> (RgbaImage, RgbaImage) {
    let quadrant = Sizing { width: Some(16), ..Sizing::default() };
    let plain = Sizing { width: Some(32), ratio: Some(0.25), ..Sizing::default() };
    let quadrant = size::fit(image, Mode::Quadrant, &quadrant).unwrap();
    let plain = size::fit(image, Mode::Halfblock, &plain).unwrap();
    assert_eq!(quadrant.image.dimensions(), (32, 16));
    assert_eq!(plain.image.dimensions(), (32, 16));
    (quadrant.image.into_owned(), plain.image.into_owned())
}

#[test]
fn quadrant_sharpens_an_opaque_resampled_image() {
    let image = halves(Rgba([100, 100, 100, 255]), Rgba([150, 150, 150, 255]));

    let (quadrant, plain) = quadrant_and_plain(&image);

    assert_eq!(quadrant, imageops::unsharpen(&plain, 1.0, 2));
    assert_ne!(quadrant, plain);
}

#[test]
fn quadrant_leaves_an_image_with_transparency_unsharpened() {
    let image = halves(Rgba([100, 100, 100, 255]), Rgba([0, 0, 0, 0]));

    let (quadrant, plain) = quadrant_and_plain(&image);

    assert_eq!(quadrant, plain);
}

#[test]
fn quadrant_uses_an_image_at_its_size_unchanged() {
    // 16 columns at ratio 1 is 16×8 cells of 2×2 pixels: the image's own 32×16.
    let image = RgbaImage::from_fn(32, 16, |x, _| Rgba([(x * 8) as u8, 0, 0, 255]));
    let sizing = Sizing { width: Some(16), ratio: Some(1.0), ..Sizing::default() };

    let fit = size::fit(&image, Mode::Quadrant, &sizing).unwrap();

    assert!(matches!(fit.image, Cow::Borrowed(_)));
}

#[test]
fn quadrant_full_width_is_half_the_pixels() {
    let sizing = Sizing { full: true, ..Sizing::default() };

    assert_eq!(cells(&image(200, 100), Mode::Quadrant, &sizing), (100, 25));
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test --test size quadrant`
Expected: `quadrant_sharpens_an_opaque_resampled_image` FAILS on its first assertion (the image is not sharpened); the other three pass.

- [ ] **Step 3: Sharpen in `fit`**

In `src/size.rs`: `const SHARPEN_SIGMA: f32 = 1.0;` and `const SHARPEN_THRESHOLD: i32 = 2;`, each with a doc line. In `fit`, on the resampled branch only, when `mode == Mode::Quadrant` and every pixel of the resampled image has alpha 255, replace it with `imageops::unsharpen(&resampled, SHARPEN_SIGMA, SHARPEN_THRESHOLD)`. The `Cow::Borrowed` branch is untouched. Extend `fit`'s doc comment with the rule.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test --test size`
Expected: PASS, all of them.

- [ ] **Step 5: Run the gate and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
git add src/size.rs tests/size.rs
git commit -m "feat(size): sharpen resampled opaque images in quadrant mode" -m "- Leave images with transparency and images used at their own size as they are"
```

---

### Task 4: Ascii by default, `--ascii-style` and `encode.ascii_style`

**Files:**
- Modify: `src/config.rs` (`Encode`, its `Default`, `read_encode`)
- Modify: `src/cli.rs` (`EncodeArgs`, `ValueEnum` for `AsciiStyle`, `encode`, `write_art`, `init`'s call of `write_art`)
- Modify: `src/init.rs` (the `[encode]` part of the full template only)
- Test: `tests/config.rs`, `tests/cli_encode.rs`, `tests/cli_init.rs`

**Interfaces:**
- Consumes: `AsciiStyle` (`Named`, `Default`, `charset()`), `render::ascii(image, style, charset)`, `render::quadrant` from Tasks 1 and 2; the golden files of Task 1.
- Produces:
  - `config::Encode { pub mode: Mode, pub width: u32, pub ratio: f64, pub ascii_style: AsciiStyle, pub charset: Option<Charset> }`; `Encode::default()` is `Mode::Ascii`, 80, 0.5, `AsciiStyle::Bright`, `None`.
  - Config key `encode.ascii_style`; a bad value's problem is `encode.ascii_style must be one of plain, bright, dense, tinted`.
  - `jaci encode --ascii-style <STYLE>`.
  - `fn write_art(fit: &Fit<'_>, mode: Mode, style: AsciiStyle, charset: Option<&Charset>, path: &Path) -> Result<(), CliError>` in `src/cli.rs`: the one place that falls back to `style.charset()` when no charset is given.

- [ ] **Step 1: Write the failing tests**

`tests/config.rs`:
- `missing_config_gives_defaults` expects `mode: Mode::Ascii`, `ascii_style: AsciiStyle::Bright`, `charset: None`.
- `every_key_loads_its_value` adds the line `ascii_style = "tinted"` under `[encode]` and expects `ascii_style: AsciiStyle::Tinted`, `charset: Some(Charset::new(" #").unwrap())`.
- New:

```rust
#[test]
fn bad_ascii_style_names_key() {
    let (path, message) = load_error("[encode]\nascii_style = \"neon\"\n");

    assert_eq!(
        message,
        format!(
            "jaci: config {}: line 2: encode.ascii_style must be one of plain, bright, dense, tinted",
            path.display()
        )
    );
}
```

`tests/cli_encode.rs`:
- `options_pick_mode_and_ratio` adds `"--ascii-style", "plain"` to its arguments; `config_picks_mode_and_ratio` adds `ascii_style = 'plain'` to its config. Both still compare with `ASCII_GOLDEN`.
- New, with a helper for the other golden files:

```rust
fn golden(name: &str) -> String {
    read(&Path::new(env!("CARGO_MANIFEST_DIR")).join("test_fixtures").join(name))
}

#[test]
fn default_is_bright_ascii() {
    let env = Env::new();

    // 4 columns at ratio 1 is the fixture's own 4×4 pixels, unresampled.
    let output = env.jaci(&["encode", "--ratio", "1", "-w", "4", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.art_path()), golden("4x4-ascii-bright.ansi"));
}

#[test]
fn ascii_style_from_config_then_option() {
    let env = Env::with_config("[encode]\nascii_style = 'tinted'\nratio = 1\nwidth = 4\n");

    for (args, name) in [
        (&[][..], "4x4-ascii-tinted.ansi"),
        (&["--ascii-style", "dense"][..], "4x4-ascii-dense.ansi"),
    ] {
        let output = env.jaci(&[&["encode"], args, &[IMAGE]].concat());

        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        assert_eq!(read(&env.art_path()), golden(name));
    }
}

#[test]
fn charset_beats_the_dense_style() {
    for (config, args) in [
        ("", &["--ascii-style", "dense", "--charset", "#"][..]),
        ("[encode]\ncharset = '#'\n", &["--ascii-style", "dense"][..]),
    ] {
        let env = Env::with_config(config);

        let output = env.jaci(&[&["encode"], args, &[IMAGE]].concat());

        assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
        let glyphs = visible(&read(&env.art_path()));
        assert!(glyphs.contains('#'), "{glyphs:?}");
        assert!(glyphs.chars().all(|c| ['#', ' ', '\n'].contains(&c)), "{glyphs:?}");
    }
}

#[test]
fn ascii_style_is_ignored_outside_ascii() {
    let env = Env::new();

    // 4 columns at ratio 0.5 is 4×2 halfblock cells: the fixture unresampled.
    let output = env.jaci(&["encode", "-m", "halfblock", "--ascii-style", "tinted", "-w", "4", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.art_path()), golden("4x4-halfblock.ansi"));
}

#[test]
fn quadrant_mode_from_the_option() {
    let env = Env::new();

    // 2 columns at ratio 1 is 2×2 quadrant cells: the fixture unresampled.
    let output = env.jaci(&["encode", "-m", "quadrant", "--ratio", "1", "-w", "2", IMAGE]);

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert_eq!(read(&env.art_path()), golden("4x4-quadrant.ansi"));
}

#[test]
fn unknown_ascii_style_is_a_usage_error() {
    let env = Env::new();

    let output = env.jaci(&["encode", "--ascii-style", "neon", IMAGE]);

    assert_eq!(output.status.code(), Some(2));
    assert!(!env.art_path().exists());
}
```

`tests/cli_init.rs`:
- Every `assert_full_config(&env, Mode::Halfblock, …)` becomes `Mode::Ascii`, except in tests that answer the mode prompt themselves.
- In `COMMENTED_KEYS`, `"charset"` becomes `"ascii_style"` (the count stays 10), and `assert_full_config` also asserts that a comment line starts with `# charset = `.
- `interactive_init_asks_for_image` expects `"Render mode (ascii, halfblock, quadrant, braille) [ascii]: "`.

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test --test config --test cli_encode --test cli_init`
Expected: FAIL to compile: `Encode` has no field `ascii_style`.

- [ ] **Step 3: Implement the config, the option and the template**

- `src/config.rs`: the `Encode` fields and default above; `read_encode` reads `"ascii_style"` with the same `named(value, "encode.ascii_style")?` helper as `"mode"`, and stores the charset as `Some(…)`.
- `src/cli.rs`:
  - `EncodeArgs` gains, after `mode`:

    ```rust
    /// How ascii mode picks and colours its characters [default:
    /// [encode].ascii_style, else bright].
    #[arg(long, value_name = "STYLE")]
    ascii_style: Option<AsciiStyle>,
    ```

    with `impl ValueEnum for AsciiStyle` built from `Named`, as `Mode`'s is. The `mode` help reads `[default: [encode].mode, else ascii]`; the `charset` help reads `[default: [encode].charset, else the ascii style's own]`.
  - `encode` passes `args.ascii_style.unwrap_or(config.encode.ascii_style)` and `args.charset.as_ref().or(config.encode.charset.as_ref())` to `write_art`.
  - `write_art` takes the signature above and, for `Mode::Ascii`, renders with the given charset or else `style.charset()`.
  - `init` passes `encode.ascii_style` and `encode.charset.as_ref()`.
- `src/init.rs`, in `full`, the `[encode]` section becomes (the other sections unchanged):

    ```text
    [encode]
    # The render mode: {modes}.
    mode = {mode}
    # How ascii mode picks and colours its characters: {styles}.
    ascii_style = {style}
    # The art's width in columns: a whole number of at least 1.
    width = {width}
    # Character width ÷ height, which keeps the art's proportions: a number above 0.
    ratio = {ratio}
    # The characters ascii mode draws with, darkest first; left out, the ascii
    # style's own.
    # charset = {charset}
    ```

    with `styles = AsciiStyle::one_of()`, `style = toml_value(encode.ascii_style.name())` and `charset` still `toml_value(DEFAULT_CHARSET)`.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test --test config --test cli_encode --test cli_init --test e2e`
Expected: PASS.

- [ ] **Step 5: Run the gate and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
git add src/config.rs src/cli.rs src/init.rs tests/config.rs tests/cli_encode.rs tests/cli_init.rs
git commit -m "feat(encode): default to bright ascii and add --ascii-style" -m "- Add encode.ascii_style and make encode.charset optional, the style's own when left out
- Make ascii the default render mode"
```

---

### Task 5: `jaci init -i` asks for the ascii style

**Files:**
- Modify: `src/init.rs` (`Settings`, `Settings::encode`, `InitError`, `ask_settings`, a `parse_style` beside `parse_mode`, the unit tests)
- Test: `tests/cli_init.rs`

**Interfaces:**
- Consumes: `AsciiStyle` and `Named`; `config::Encode` from Task 4; the full template of Task 4, which already writes `encode.ascii_style`.
- Produces:
  - `init::Settings { pub mode: Mode, pub ascii_style: AsciiStyle, pub width: u32, pub shown: [bool; BLOCK_COUNT] }`; `Settings::encode()` carries the style.
  - `InitError::InvalidStyle(String)`, displayed as `jaci: '<answer>' is not a valid ascii style`.
  - The prompt `Ascii style (plain, bright, dense, tinted) [bright]: `, asked right after the mode prompt and only when the mode answered is ascii.

- [ ] **Step 1: Write the failing tests**

`tests/cli_init.rs`:
- `assert_full_config` takes the style too: `fn assert_full_config(env: &Env, mode: Mode, style: AsciiStyle, width: u32, contents: &[&str])`, with `ascii_style: style` in the expected `Encode`; existing callers pass `AsciiStyle::Bright`.
- `interactive_init_asks_for_image` feeds ten empty lines after the image and expects, between the mode and the width prompts, `"Ascii style (plain, bright, dense, tinted) [bright]: "`.
- `image_answer_expands_home` and `missing_image_stops_init_before_the_questions` feed ten empty lines.
- `given_image_skips_image_prompt` keeps its answers (`braille`, `40`, an empty line, `n`, five empty lines) and additionally asserts `!stdout.contains("Ascii style")`; it passes `AsciiStyle::Bright` to `assert_full_config`.
- `too_large_width_writes_nothing` keeps `halfblock\n20000\n` and seven empty lines.
- New:

```rust
#[test]
fn interactive_init_asks_the_style_for_ascii() {
    let env = Env::new();

    let output = env.jaci_with_input(
        &["init", "-i", IMAGE],
        &format!("ascii\ntinted\n40\n{}", "\n".repeat(7)),
    );

    assert_eq!(output.status.code(), Some(0), "{}", stderr(&output));
    assert!(
        stdout(&output).starts_with(concat!(
            "Render mode (ascii, halfblock, quadrant, braille) [ascii]: ",
            "Ascii style (plain, bright, dense, tinted) [bright]: ",
            "Width in columns [80]: ",
        )),
        "{}",
        stdout(&output)
    );
    let text = env.config_text();
    assert!(text.lines().any(|line| line == r#"ascii_style = "tinted""#), "{text}");
    assert_full_config(&env, Mode::Ascii, AsciiStyle::Tinted, 40, &FULL_BLOCKS);
    assert_eq!(read_art(&env.art_path()).columns(), 40);
}

#[test]
fn invalid_style_stops_init() {
    let env = Env::new();

    let output = env.jaci_with_input(&["init", "-i", IMAGE], "ascii\nneon\n");

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        format!("jaci: 'neon' is not a valid ascii style\n{HELP_LINE}\n")
    );
    assert!(!env.config_folder().exists());
}
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test --test cli_init`
Expected: `interactive_init_asks_for_image`, `interactive_init_asks_the_style_for_ascii` and `invalid_style_stops_init` FAIL: no style prompt is printed, `tinted` is read as the width.

- [ ] **Step 3: Implement the prompt**

In `src/init.rs`: `Settings` and `Settings::encode` as above (`Settings::default()` takes the style from `Encode::default()`); `InitError::InvalidStyle` beside `InvalidMode`, in `Display` and in `source`'s list; `fn parse_style(answer: &str, default: AsciiStyle) -> Result<AsciiStyle, InitError>` with the shape of `parse_mode`; `ask_settings` asks the style after the mode when the mode is `Mode::Ascii`, building the question from `AsciiStyle::names()` and the default's `name()`, and otherwise keeps the default style. Add `parse_style` cases to the existing unit tests `empty_answers_take_the_defaults`, `answers_parse` and `invalid_answers_name_themselves`. Update `ask_settings`'s doc comment and the `-i` help line in `src/cli.rs` to mention the ascii style.

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test --test cli_init --lib`
Expected: PASS.

- [ ] **Step 5: Run the gate, check the real output and commit**

```bash
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test && RUSTDOCFLAGS="-D warnings" cargo doc --no-deps
cargo build --release
for style in plain bright dense tinted; do ./target/release/jaci encode ~/Pictures/jeff-iron-man-small.png --ascii-style $style -o /tmp/jaci-$style.ansi; done
./target/release/jaci encode ~/Pictures/jeff-iron-man-small.png -m quadrant -o /tmp/jaci-quadrant.ansi
git add src/init.rs src/cli.rs tests/cli_init.rs
git commit -m "feat(init): ask for the ascii style in interactive init" -m "- Ask it right after the render mode, and only for ascii"
```

Expected: the gate is clean; the five art files exist, each 80 columns by 33 rows (`wc -l` gives 33). They are for Jeff to `cat` in Orca and the Mac Terminal.
