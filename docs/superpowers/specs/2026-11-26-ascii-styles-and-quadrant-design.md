# Ascii styles and a quadrant mode: design

Date: 2026-11-26. Builds on jaci as it stands on `feat/jaci` (pull request #12).

## Intent and why

Jeff encoded a photo at the default settings and found the art "too blocky/blur, I can barely see it". The size is right (80 columns, same proportions); the picture inside it is not readable enough. He then looked at every mode in his own terminals and decided:

- ascii is the look he wants by default, but today's ascii is too dark;
- he wants to choose among several ascii looks, including today's;
- the block art should get a sharper option that keeps the same size.

Success: `jaci encode photo.png` with no options gives art in which he recognises the photo in Orca and the Mac Terminal, at the size it has today.

## Goals

- Ascii is the default render mode.
- Ascii has four styles, chosen by an option or a config key: `plain`, `bright` (default), `dense`, `tinted`.
- A new `quadrant` render mode: twice the horizontal detail of halfblock at the same art size.
- Art width, height and proportions are unchanged for every existing mode.

## Non-goals

- Sextant characters: they streak in Orca and show as boxes in the Mac Terminal (seen in Jeff's screenshots).
- Sharpening or brightening halfblock or braille: their output stays byte for byte what it is today.
- Dithering, edge-shaped characters, or any other new ascii technique.
- A way to tune the brightness numbers from the config.

## Anti-goals

- Art that changes size or proportions.
- A default that draws badly in a common terminal.
- Losing today's ascii look: it stays as `plain`.

## Constraints

- Rust 1.87, edition 2024, the `image` 0.25 crate already in use; no new dependency.
- Output is deterministic across macOS and Linux, so golden files can pin it: the brightness maths uses only integer arithmetic, `+ − × ÷` and `sqrt` on `f64` (all correctly rounded), never `powf`.
- A transparent pixel is a space with no escape, and every coloured cell ends in `ESC[0m`, as today.
- jaci is not released: the default mode, the config type and `render::ascii`'s signature may change freely.

## Approaches

Three ways to make the picture readable at the same size were on the table.

1. **More detail per cell with block characters.** Each cell is split into more parts, drawn with two colours.
   - *Sextant* (2×3 per cell): the sharpest in simulation, but it does not draw in either of Jeff's terminals. Dropped.
   - *Quadrant* (2×2 per cell): uses the same block family halfblock already draws cleanly. Twice the horizontal detail; a cell still holds only two colours.
   - *Only sharpen halfblock*: no new mode, a small gain. Dropped.
2. **Make ascii readable.** Today a dark pixel gets a sparse character *and* a dark colour, so it is dimmed twice. Stretch the contrast and draw the characters brighter (`bright`); add a longer character set (`dense`); add a dim background (`tinted`).
3. **More columns.** `-w 160` fixes it today, but it changes the size. Jeff ruled it out.

Jeff picked: ascii as the default with all four styles selectable and `bright` the default style (2), plus quadrant as an opt-in mode (1). Quadrant goes in without his having seen it in Orca; it is opt-in, so a bad result costs nothing by default.

## What Jeff decided

- The default mode is ascii.
- All four ascii styles exist and are selectable; `bright` is the default.
- `tinted` may paint a background behind the art. This lifts the product brief's "no background fill" for this one style, which he chose after seeing it.
- Quadrant is built, opt-in.

## The design

### Modes

The modes, in the order help text, messages and prompts list them: `ascii`, `halfblock`, `quadrant`, `braille`. `ascii` is the default wherever `halfblock` was: the built-in default, the `-m` help, the full config and the interactive prompt.

### Ascii styles

`--ascii-style <plain|bright|dense|tinted>` on `jaci encode`, and `ascii_style` under `[encode]` in `config.toml`. The option beats the config, which beats the default `bright`. In any other mode the style is ignored, as `--charset` is today. A bad config value reads `encode.ascii_style must be one of plain, bright, dense, tinted`.

The characters come from `--charset`, else `[encode].charset`, else the style's own set:

- `plain`, `bright`, `tinted`: ` .:-=+*#%@` (10 characters, as today).
- `dense`: the standard 70-level ramp, darkest first:
  ``` .'`^",:;Il!i><~+_-?][}{1)(|\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$```

So `[encode].charset` becomes optional in the config's type: left out, the style decides.

**plain** is today's rule, unchanged: the character at ⌊L × n ÷ 256⌋ with L = 0.299R + 0.587G + 0.114B, drawn in the pixel's own colour. `test_fixtures/4x4-ascii.ansi` stays its golden file.

**bright**, for the opaque pixels of the fitted image:

1. Luminance in thousandths: `Y = 299R + 587G + 114B` (0 to 255 000), an integer.
2. Sort the Y values of all opaque pixels. With `k` of them, `lo = sorted[⌊2k ÷ 100⌋]` and `hi = sorted[⌊98k ÷ 100⌋]`.
3. `t = clamp((Y − lo) ÷ (hi − lo), 0, 1)`. When `hi = lo` (a flat image), `t = Y ÷ 255 000`.
4. `l = sqrt(sqrt(t × t × t))`, which is t^0.75: it lifts the mid-tones.
5. The character is at index `min(⌊l × n⌋, n − 1)` of the n characters.
6. The colour keeps the pixel's hue at brightness `v = 0.6 + 0.4 × l`: each channel is `round(c ÷ max(R, G, B) × v × 255)`; for a black pixel (max = 0) every channel is `round(v × 255)`. `round` is half away from zero.
7. The cell is `ESC[38;2;R;G;Bm<char>ESC[0m` with that colour.

**dense** is `bright` with the 70-character set as its default characters.

**tinted** is `bright` plus a background of the pixel's own colour at 40%: each channel `round(c × 0.4)`. The cell is `ESC[38;2;…mESC[48;2;…m<char>ESC[0m`.

A transparent pixel is a space in every style and takes no part in the percentiles. An image with no opaque pixel is all spaces.

### Quadrant mode

`-m quadrant` / `mode = "quadrant"`. A cell covers 2×2 pixels, so `--full` is the image width ÷ 2 and the image is resampled to columns × 2 by rows × 2.

Quarter bits: upper-left 1, upper-right 2, lower-left 4, lower-right 8. The character for a set of quarters, by that sum from 0 to 15: ` ▘▝▀▖▌▞▛▗▚▐▜▄▙▟█`.

- **No opaque pixel:** a space, no escape.
- **All four opaque:** split the four pixels into a foreground and a background group. Try the seven splits whose foreground is a sum from 1 to 7 (the lower-right pixel is always background, which covers every two-group split once). Pick the split with the smallest total squared distance from each pixel to its group's mean colour, compared exactly in integers; on a tie the lowest sum wins. Draw `ESC[38;2;<fg>mESC[48;2;<bg>m<char>ESC[0m` with each group's mean colour, rounded half up per channel as braille does.
- **One to three opaque:** the opaque quarters form the character, in their mean colour, with no background: `ESC[38;2;<mean>m<char>ESC[0m`.

Pixels past the image's edge count as transparent, as in the other modes.

**Sharpening.** In quadrant mode, an image that was resampled and is fully opaque is sharpened afterwards with `image::imageops::unsharpen(&image, 1.0, 2)`. An image used at its own size is not touched (the existing rule "an image already that size is used unchanged" holds), and an image with any transparent or translucent pixel is not sharpened.

### Init

- `jaci init <image>` writes `mode = "ascii"` and `ascii_style = "bright"`, each with a comment line above it.
- The full config no longer sets `charset`; it shows it as a commented-out example with its comment, the way `command` and `color` are shown, so the style's own characters apply.
- `jaci init -i` asks `Render mode (ascii, halfblock, quadrant, braille) [ascii]: `, then, only when the answer is ascii, `Ascii style (plain, bright, dense, tinted) [bright]: `, then the width and the variables as today. An answer that names no style stops init with `jaci: '<answer>' is not a valid ascii style`, exit 1, nothing written.
- `--minimal` writes no `[encode]` keys, so it follows the new defaults.

## Calls I made (not Jeff's words)

- Option and key names: `--ascii-style` and `encode.ascii_style`. `style` alone already means bold/dim on text blocks.
- Today's look is named `plain`.
- The brightness numbers (2nd and 98th percentile, exponent 0.75, brightness 60–100%, tint 40%) are the ones in the prototype he looked at.
- `dense` uses the standard 70-character ramp. The prototype he saw had 69: it lacked the backslash.
- Mode order `ascii, halfblock, quadrant, braille`.
- `--ascii-style` with a non-ascii mode is ignored, not an error.
- The full config comments `charset` out instead of setting it, because a set `charset` would override `dense`.
- Quadrant sharpening skips images with any transparency: sharpening would smear the hidden colour of transparent pixels into the edges, the class of defect the resampler took five review rounds to get right.
- Sharpening strength: sigma 1.0, threshold 2, as in the prototype.
- From the builder check, settled here:
  - The full config writes `ascii_style` whatever mode `init -i` picked.
  - "Fully opaque", for sharpening, is judged on the resampled image: every alpha is 255.
  - The `f64` operations of the brightness steps run in the order written, left to right: `c ÷ max × v × 255`, `0.6 + 0.4 × l`, `c × 0.4`.

## Left to the builder

- Where the style's types and helpers live inside `src/render.rs`, and how the per-image percentiles are passed to the per-cell code.
- The integer form of the quadrant split comparison.
- How the fixtures' golden files are produced, as long as they are computed independently of the renderer (as the existing goldens were) and the method is recorded.
- Test names and layout, following the existing test files.

## Checks that prove it

- Golden art files for `bright`, `dense`, `tinted` and `quadrant` on `test_fixtures/4x4.png`, computed outside the renderer.
- `plain` still matches `test_fixtures/4x4-ascii.ansi`; halfblock and braille still match theirs.
- `jaci encode test_fixtures/4x4.png` with no options writes ascii art in the `bright` style, 80 by 40.
- Option beats config for `--ascii-style`; a bad `ascii_style` in the config names the key and its values.
- `dense` uses its 70 characters unless a charset is given.
- Quadrant: `--full` on a 200-pixel-wide image is 100 columns; an opaque resampled image comes out sharpened, a transparent one and an unresized one do not.
- `jaci init -i` asks the style only for ascii, and the config it writes loads back with the answers.
- The whole suite passes on macOS and in the Linux container.
