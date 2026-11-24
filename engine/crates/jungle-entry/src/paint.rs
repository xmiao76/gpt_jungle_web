#![cfg(feature = "arena")]
use crate::Snapshot;
use font8x8::{BASIC_FONTS, UnicodeFonts};
use jungle_core::{RED_DEN, Side, Terrain, terrain_at, trap_owner};
const WIDTH: usize = 448;
const HEIGHT: usize = 576;
const ATLAS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/atlas.rgba"));
const NAMES: [&str; 8] = [
    "Rat", "Cat", "Dog", "Wolf", "Leopard", "Tiger", "Lion", "Elephant",
];
struct Canvas(Vec<u8>);
impl Canvas {
    fn rect(&mut self, x: usize, y: usize, w: usize, h: usize, color: [u8; 3]) {
        for row in y..(y + h).min(HEIGHT) {
            for col in x..(x + w).min(WIDTH) {
                let p = 4 * (row * WIDTH + col);
                self.0[p..p + 3].copy_from_slice(&color);
                self.0[p + 3] = 255;
            }
        }
    }
    fn image(&mut self, id: usize, x: usize, y: usize) {
        let (start, n) = if id < 6 {
            (id * 64 * 64 * 4, 64)
        } else {
            (6 * 64 * 64 * 4 + (id - 6) * 50 * 50 * 4, 50)
        };
        for row in 0..n {
            for col in 0..n {
                let a = start + 4 * (row * n + col);
                let p = 4 * ((y + row) * WIDTH + x + col);
                let alpha = u32::from(ATLAS[a + 3]);
                for c in 0..3 {
                    self.0[p + c] = ((u32::from(ATLAS[a + c]) * alpha
                        + u32::from(self.0[p + c]) * (255 - alpha)
                        + 127)
                        / 255) as u8;
                }
                self.0[p + 3] = 255;
            }
        }
    }
    fn label(&mut self, s: &str, cx: usize, y: usize, color: [u8; 3], scale: usize) {
        let w = s.len() * 7 * scale;
        let mut x = cx.saturating_sub(w / 2);
        for ch in s.chars() {
            if let Some(glyph) = BASIC_FONTS.get(ch) {
                for (dy, byte) in glyph.iter().enumerate() {
                    for dx in 0..7 {
                        if byte & (1 << dx) != 0 {
                            self.rect(x + dx * scale, y + dy * scale, scale, scale, color);
                        }
                    }
                }
            }
            x += 7 * scale;
        }
    }
    fn piece(&mut self, code: u8, x: usize, y: usize) {
        if !(1..=16).contains(&code) {
            return;
        }
        self.image(5 + usize::from(code), x + 7, y + 1);
        let name = NAMES[usize::from((code - 1) % 8)];
        let w = name.len() * 7 + 6;
        let accent = if code <= 8 {
            [98, 173, 255]
        } else {
            [255, 139, 119]
        };
        self.rect(x + 32 - w / 2, y + 49, w, 13, [24, 36, 30]);
        self.rect(x + 32 - w / 2, y + 61, w, 1, accent);
        self.label(name, x + 32, y + 52, [255, 245, 218], 1);
    }
}
pub fn frame(s: &Snapshot, flipped: bool, progress: u16, fixture: bool) -> Vec<u8> {
    let mut c = Canvas(vec![255; WIDTH * HEIGHT * 4]);
    let coords = |i: usize| {
        let d = if flipped { 62 - i } else { i };
        ((d % 7) * 64, (d / 7) * 64)
    };
    let last = s.history.last().filter(|m| m.src < 63 && m.dst < 63);
    for i in 0..63 {
        let (x, y) = coords(i);
        let sq = i as u8;
        let tile = match terrain_at(sq) {
            Terrain::Land => 0,
            Terrain::Water => 1,
            Terrain::Trap => {
                if trap_owner(sq) == Some(Side::Blue) {
                    2
                } else {
                    3
                }
            }
            Terrain::Den => {
                if sq == RED_DEN {
                    5
                } else {
                    4
                }
            }
        };
        c.rect(
            x,
            y,
            64,
            64,
            if fixture { [41, 55, 81] } else { [31, 60, 43] },
        );
        c.image(tile, x, y);
        c.rect(x, y, 64, 1, [93, 75, 44]);
        c.rect(x, y, 1, 64, [93, 75, 44]);
        if last.is_some_and(|m| m.src == sq || m.dst == sq) {
            let gold = [246, 215, 131];
            c.rect(x + 2, y + 2, 60, 2, gold);
            c.rect(x + 2, y + 60, 60, 2, gold);
            c.rect(x + 2, y + 2, 2, 60, gold);
            c.rect(x + 60, y + 2, 2, 60, gold);
        }
    }
    for (i, &code) in s.board.iter().take(63).enumerate() {
        if code != 0 {
            let (mut x, mut y) = coords(i);
            if let Some(m) = last
                && usize::from(m.dst) == i
                && progress < 1000
            {
                let (ox, oy) = coords(usize::from(m.src));
                let t = f32::from(progress) / 1000.0;
                x = (ox as f32 + (x as f32 - ox as f32) * t).round() as usize;
                y = (oy as f32 + (y as f32 - oy as f32) * t).round() as usize;
            }
            c.piece(code, x, y);
        }
    }
    if fixture {
        for px in c.0.chunks_exact_mut(4) {
            let l = ((u16::from(px[0]) + u16::from(px[1]) + u16::from(px[2])) / 3) as u8;
            px[0] = l / 2;
            px[1] = l;
            px[2] = l.saturating_add(30);
        }
    }
    if s.status != "playing" {
        c.rect(50, 255, 348, 66, [20, 43, 37]);
        c.rect(50, 255, 348, 3, [224, 190, 116]);
        c.label(
            match s.winner {
                Some(Side::Red) => "Red wins",
                Some(Side::Blue) => "Blue wins",
                None => "Draw",
            },
            224,
            267,
            [255, 235, 179],
            2,
        );
        c.label(&s.reason, 224, 296, [225, 237, 219], 1);
    }
    c.0
}
