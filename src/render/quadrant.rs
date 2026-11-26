//! Quadrant mode: one character per 2×2 pixels, drawn in the two colours
//! that fit the four pixels best.

use image::RgbaImage;

use super::{Mode, Rgb, mean, opaque, push_cell, render_cells};
use crate::ansi::{self, RESET};

/// The quadrant characters by the sum of their quarters' bits: upper-left 1,
/// upper-right 2, lower-left 4, lower-right 8.
const QUADRANTS: [char; 16] = [
    ' ', '▘', '▝', '▀', '▖', '▌', '▞', '▛', '▗', '▚', '▐', '▜', '▄', '▙', '▟', '█',
];
/// Each quarter's pixel offset in the 2×2 cell, in the order of the bits.
const QUARTERS: [(u32, u32); 4] = [(0, 0), (1, 0), (0, 1), (1, 1)];
/// The bits of a cell whose four quarters are opaque.
const FULL: usize = 0b1111;

/// Renders `image` with one cell per 2×2 pixels. A cell with a transparent
/// pixel raises the quarters of its opaque ones, in their rounded mean
/// colour. A cell of four opaque pixels splits them into a foreground and a
/// background group, the split whose groups vary least in colour, and draws
/// the foreground's quarters in its rounded mean colour over the
/// background's.
pub fn quadrant(image: &RgbaImage) -> Vec<String> {
    render_cells(image, Mode::Quadrant.cell_size(), |row, x, y| {
        let pixels = QUARTERS.map(|(dx, dy)| opaque(image, x + dx, y + dy));
        let opaque_bits = (0..pixels.len())
            .filter(|&quarter| pixels[quarter].is_some())
            .fold(0, |bits, quarter| bits | 1 << quarter);
        match opaque_bits {
            0 => row.push(' '),
            FULL => {
                // The lower-right quarter stays in the background, so each
                // split is tried once; ties go to the lowest foreground bits.
                let foreground_bits = (1..=FULL >> 1)
                    .min_by_key(|&bits| {
                        Group::of(&pixels, bits).spread() + Group::of(&pixels, FULL ^ bits).spread()
                    })
                    .expect("there is a split to try");
                ansi::push_foreground(row, Group::of(&pixels, foreground_bits).mean());
                ansi::push_background(row, Group::of(&pixels, FULL ^ foreground_bits).mean());
                row.push(QUADRANTS[foreground_bits]);
                row.push_str(RESET);
            }
            bits => push_cell(row, Group::of(&pixels, bits).mean(), QUADRANTS[bits]),
        }
    })
}

/// The opaque pixels of a cell's chosen quarters, summed.
struct Group {
    count: u32,
    /// The sum of each channel.
    sums: [u32; 3],
    /// The sum of every channel's square.
    squares: u32,
}

impl Group {
    /// The group of the opaque `pixels` whose quarter's bit is in `bits`.
    fn of(pixels: &[Option<Rgb>; 4], bits: usize) -> Self {
        let mut group = Self {
            count: 0,
            sums: [0; 3],
            squares: 0,
        };
        for (quarter, pixel) in pixels.iter().enumerate() {
            let Some(colour) = pixel else { continue };
            if bits & 1 << quarter == 0 {
                continue;
            }
            group.count += 1;
            for (sum, &channel) in group.sums.iter_mut().zip(colour) {
                let channel = u32::from(channel);
                *sum += channel;
                group.squares += channel * channel;
            }
        }
        group
    }

    /// The rounded mean colour of the group, which is not empty.
    fn mean(&self) -> Rgb {
        mean(self.sums, self.count)
    }

    /// Six times the group's summed squared distance from its mean colour:
    /// 6 × Σ(c²) − (6 ÷ n) × Σ over the channels of (Σ c)², exact in integers
    /// for the one to three pixels a split's group holds.
    fn spread(&self) -> u32 {
        let squared_sums: u32 = self.sums.iter().map(|sum| sum * sum).sum();
        6 * self.squares - 6 / self.count * squared_sums
    }
}
