//! A canvas whose every coordinate wraps, so whatever is drawn on it tiles.

use image::RgbaImage;

use crate::color::{lerp, luminance, scale, to_srgb8, Rgb};
use crate::noise::hash01;

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    px: Vec<Rgb>,
}

impl Canvas {
    pub fn filled(w: usize, h: usize, c: Rgb) -> Self {
        Canvas { w, h, px: vec![c; w * h] }
    }

    fn index(&self, x: i64, y: i64) -> usize {
        let x = x.rem_euclid(self.w as i64) as usize;
        let y = y.rem_euclid(self.h as i64) as usize;
        y * self.w + x
    }

    pub fn get(&self, x: i64, y: i64) -> Rgb {
        self.px[self.index(x, y)]
    }

    /// Moves the pixel toward `c` by `a` in [0, 1].
    pub fn blend(&mut self, x: i64, y: i64, c: Rgb, a: f32) {
        let i = self.index(x, y);
        self.px[i] = lerp(self.px[i], c, a);
    }

    /// Rewrites every pixel from its unit coordinates and current color.
    pub fn map(&mut self, f: impl Fn(f32, f32, Rgb) -> Rgb) {
        for y in 0..self.h {
            let v = y as f32 / self.h as f32;
            for x in 0..self.w {
                let u = x as f32 / self.w as f32;
                let i = y * self.w + x;
                self.px[i] = f(u, v, self.px[i]);
            }
        }
    }

    #[allow(dead_code)]
    /// A disc of radius `r` pixels at (`cx`, `cy`), edge softened over one
    /// pixel.
    pub fn disc(&mut self, cx: f32, cy: f32, r: f32, c: Rgb, a: f32) {
        let x0 = (cx - r - 1.0).floor() as i64;
        let x1 = (cx + r + 1.0).ceil() as i64;
        let y0 = (cy - r - 1.0).floor() as i64;
        let y1 = (cy + r + 1.0).ceil() as i64;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
                let k = (r - d + 0.5).clamp(0.0, 1.0);
                if k > 0.0 {
                    self.blend(x, y, c, a * k);
                }
            }
        }
    }

    #[allow(dead_code)]
    /// A one-pixel line between two pixel centres.
    pub fn line(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, c: Rgb, a: f32) {
        let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
        let (sx, sy) = ((x1 - x0).signum(), (y1 - y0).signum());
        let (mut x, mut y, mut err) = (x0, y0, dx + dy);
        loop {
            self.blend(x, y, c, a);
            if x == x1 && y == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x += sx;
            }
            if e2 <= dx {
                err += dx;
                y += sy;
            }
        }
    }

    /// Flattens brightness drift wider than `radius` pixels by dividing each
    /// pixel by the local mean luminance, so no region reads lighter than
    /// another once the tile repeats.
    pub fn equalize(&mut self, radius: usize) {
        let lum: Vec<f32> = self.px.iter().map(|c| luminance(*c)).collect();
        let mean = lum.iter().sum::<f32>() / lum.len() as f32;
        let local = self.blur(&self.blur(&lum, radius), radius);
        for (p, l) in self.px.iter_mut().zip(local) {
            *p = scale(*p, (mean / l.max(1e-4)).clamp(0.5, 2.0));
        }
    }

    /// Wrapped box blur, separable.
    fn blur(&self, src: &[f32], radius: usize) -> Vec<f32> {
        let r = radius as i64;
        let n = (2 * r + 1) as f32;
        let mut mid = vec![0.0; src.len()];
        for y in 0..self.h as i64 {
            for x in 0..self.w as i64 {
                let s: f32 = (-r..=r).map(|d| src[self.index(x + d, y)]).sum();
                mid[self.index(x, y)] = s / n;
            }
        }
        let mut out = vec![0.0; src.len()];
        for y in 0..self.h as i64 {
            for x in 0..self.w as i64 {
                let s: f32 = (-r..=r).map(|d| mid[self.index(x, y + d)]).sum();
                out[self.index(x, y)] = s / n;
            }
        }
        out
    }

    /// Scales the whole image so its mean luminance is `target`, so a
    /// texture built from dark gaps and light strokes still averages to the
    /// ramp stop it stands in for.
    pub fn set_mean_luminance(&mut self, target: f32) {
        let mean = self.px.iter().map(|c| luminance(*c)).sum::<f32>() / self.px.len() as f32;
        let k = target / mean.max(1e-4);
        for p in self.px.iter_mut() {
            *p = scale(*p, k);
        }
    }

    /// Per-pixel brightness jitter of plus or minus `amount`.
    pub fn grain(&mut self, amount: f32, seed: u64) {
        for y in 0..self.h {
            for x in 0..self.w {
                let k = 1.0 + (hash01(x as i64, y as i64, seed) * 2.0 - 1.0) * amount;
                self.px[y * self.w + x] = scale(self.px[y * self.w + x], k);
            }
        }
    }

    /// Luminance contrast across the wrap seam over the strongest contrast
    /// between any two interior neighbour columns or rows, the larger of the
    /// two axes. At or under 1.0 the seam is no sharper than the image's own
    /// features; a hard seam is many times that.
    pub fn seam_ratio(&self) -> f32 {
        let (w, h) = (self.w as i64, self.h as i64);
        let d = |a: (i64, i64), b: (i64, i64)| {
            (luminance(self.get(a.0, a.1)) - luminance(self.get(b.0, b.1))).abs()
        };
        let columns: Vec<f32> = (0..w).map(|x| (0..h).map(|y| d((x, y), (x + 1, y))).sum::<f32>() / h as f32).collect();
        let rows: Vec<f32> = (0..h).map(|y| (0..w).map(|x| d((x, y), (x, y + 1))).sum::<f32>() / w as f32).collect();
        let worst = |edges: &[f32]| {
            let (seam, interior) = edges.split_last().unwrap();
            seam / interior.iter().cloned().fold(1e-6_f32, f32::max)
        };
        worst(&columns).max(worst(&rows))
    }

    pub fn to_image(&self) -> RgbaImage {
        self.tiled(1)
    }

    /// The tile repeated `n` times each way.
    pub fn tiled(&self, n: u32) -> RgbaImage {
        let (w, h) = (self.w as u32, self.h as u32);
        RgbaImage::from_fn(w * n, h * n, |x, y| {
            let c = self.get((x % w) as i64, (y % h) as i64);
            image::Rgba([to_srgb8(c[0]), to_srgb8(c[1]), to_srgb8(c[2]), 255])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seam_ratio_catches_a_gradient() {
        let mut c = Canvas::filled(64, 64, [0.0; 3]);
        c.map(|u, _, _| [u; 3]);
        assert!(c.seam_ratio() > crate::SEAM_LIMIT);
    }

    #[test]
    fn seam_ratio_passes_wrapped_noise() {
        let mut c = Canvas::filled(64, 64, [0.0; 3]);
        c.map(|u, v, _| [0.5 + 0.5 * crate::noise::fbm(u, v, 4, 3, 0.5, 1); 3]);
        assert!(c.seam_ratio() <= crate::SEAM_LIMIT);
    }
}

