//! 128x128 screen on the APEX receiver/dock (commands go to the receiver
//! itself, not routed to the mouse). Frames are RGB565 big-endian, sent
//! column-major in 56-byte chunks after an A5 "begin" handshake.

use crate::hid::{Checksum, RyChannel, Target};
use anyhow::{anyhow, Context, Result};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use image::{imageops::FilterType, AnimationDecoder, Rgba, RgbaImage};
use std::path::Path;

pub const SIZE: u32 = 128;
pub const FRAME_BYTES: usize = (SIZE * SIZE * 2) as usize;
pub const CHUNK: usize = 56;
pub const CHUNKS_PER_FRAME: usize = FRAME_BYTES.div_ceil(CHUNK);
/// App limit for receiver id 1759.
pub const MAX_FRAMES: usize = 222;

pub const BEGIN: u8 = 0xA5;
pub const DATA: u8 = 0x25;
pub const TIME_SYNC: u8 = 0x28;
pub const VERSION: u8 = 0xAD;

/// A prepared animation (or a single picture).
#[derive(Clone)]
pub struct Animation {
    /// RGB565 BE, column-major, FRAME_BYTES each.
    pub frames: Vec<Vec<u8>>,
    /// Per-frame delay in ms (1..=255; 0 for a still picture).
    pub delay_ms: u8,
    /// Small RGBA previews for the UI.
    pub previews: Vec<RgbaImage>,
}

/// Decodes a GIF/PNG/JPEG, letterboxes each frame onto 128x128 black.
pub fn load(path: &Path) -> Result<Animation> {
    let is_gif = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("gif"));
    let (mut imgs, delay) = if is_gif {
        let file = std::io::BufReader::new(std::fs::File::open(path)?);
        let dec = image::codecs::gif::GifDecoder::new(file).context("not a valid GIF")?;
        let frames = dec.into_frames().collect_frames().context("couldn't decode GIF frames")?;
        let delay = frames.first().map_or(100, |f| {
            let (n, d) = f.delay().numer_denom_ms();
            n / d.max(1)
        });
        (frames.into_iter().map(|f| f.into_buffer()).collect::<Vec<_>>(), delay)
    } else {
        (vec![image::open(path).context("couldn't open image")?.to_rgba8()], 0)
    };
    if imgs.is_empty() {
        return Err(anyhow!("image has no frames"));
    }
    if imgs.len() > MAX_FRAMES {
        // Keep the animation's length by sampling evenly.
        let step = imgs.len() as f32 / MAX_FRAMES as f32;
        imgs = (0..MAX_FRAMES).map(|i| imgs[(i as f32 * step) as usize].clone()).collect();
    }
    let canvases: Vec<RgbaImage> = imgs.iter().map(letterbox).collect();
    let delay_ms = if canvases.len() > 1 { delay.clamp(1, 255) as u8 } else { 0 };
    Ok(Animation {
        frames: canvases.iter().map(to_rgb565_columns).collect(),
        previews: canvases,
        delay_ms,
    })
}

fn letterbox(src: &RgbaImage) -> RgbaImage {
    let (w, h) = src.dimensions();
    let scale = (SIZE as f32 / w as f32).min(SIZE as f32 / h as f32);
    let (nw, nh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    let small = image::imageops::resize(src, nw, nh, FilterType::Triangle);
    let mut canvas = RgbaImage::from_pixel(SIZE, SIZE, Rgba([0, 0, 0, 255]));
    image::imageops::overlay(&mut canvas, &small, ((SIZE - nw) / 2).into(), ((SIZE - nh) / 2).into());
    canvas
}

/// RGB565 big-endian, x outer / y inner (column-major), alpha over black.
pub fn to_rgb565_columns(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::with_capacity(FRAME_BYTES);
    for x in 0..SIZE {
        for y in 0..SIZE {
            let Rgba([r, g, b, a]) = *img.get_pixel(x, y);
            let mul = |c: u8| (c as u16 * a as u16 / 255) as u8;
            let v = (u16::from(mul(r) >> 3) << 11) | (u16::from(mul(g) >> 2) << 5) | u16::from(mul(b) >> 3);
            out.extend_from_slice(&v.to_be_bytes());
        }
    }
    out
}

/// A5 payload for a full-screen upload. For a still picture `first` is the
/// target layer (0..4) and `count` is 1.
pub fn begin_payload(first: u8, count: u8, delay_ms: u8) -> Vec<u8> {
    let len = FRAME_BYTES as u32;
    let mut p = vec![0u8; 18];
    p[0] = BEGIN;
    p[1] = first;
    p[2] = count;
    p[3] = delay_ms;
    p[4..6].copy_from_slice(&(len as u16).to_le_bytes());
    // box: left, top, right, bottom low bytes then high bytes
    p[8..12].copy_from_slice(&[0, 0, SIZE as u8, SIZE as u8]);
    p[16..18].copy_from_slice(&((len >> 16) as u16).to_le_bytes());
    p
}

/// 25 payloads for one frame.
pub fn data_payloads(frame: u8, count: u8, delay_ms: u8, pixels: &[u8]) -> Vec<Vec<u8>> {
    pixels
        .chunks(CHUNK)
        .enumerate()
        .map(|(i, c)| {
            let [lo, hi] = (i as u16).to_le_bytes();
            let mut p = vec![DATA, frame, count, delay_ms, lo, hi, c.len() as u8, 0];
            p.extend_from_slice(c);
            p
        })
        .collect()
}

/// 28 payload: big-endian year, then month/day/hour/min/sec.
pub fn time_payload(year: u16, month: u8, day: u8, hour: u8, min: u8, sec: u8) -> Vec<u8> {
    let [yh, yl] = year.to_be_bytes();
    vec![TIME_SYNC, 0, 0, 0, 0, 0, 0, 0, yh, yl, month, day, hour, min, sec]
}

/// Direct channel to the receiver for screen commands; `Send`, so an upload
/// can run on a worker thread.
pub struct ScreenLink {
    chan: RyChannel,
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

impl ScreenLink {
    pub fn open(api: &hidapi::HidApi, path: &std::ffi::CStr) -> Result<Self> {
        let dev = api.open_path(path)?;
        Ok(Self { chan: RyChannel::new(Box::new(dev), Target::Direct) })
    }

    /// Firmware versions (screen, flash), if the receiver answers AD.
    pub fn version(&self) -> Option<(u16, u16)> {
        let r = self.chan.query_as(&[VERSION], Checksum::Bit7, Target::Direct).ok()?;
        (r[0] == VERSION).then(|| (u16::from_le_bytes([r[1], r[2]]), u16::from_le_bytes([r[3], r[4]])))
    }

    pub fn sync_time(&self, t: (u16, u8, u8, u8, u8, u8)) -> Result<()> {
        self.chan.write_as(&time_payload(t.0, t.1, t.2, t.3, t.4, t.5), Checksum::Bit7, Target::Direct)
    }

    /// Uploads an animation (or a picture to `layer` when it has one frame).
    /// `progress` counts sent chunks; setting `cancel` stops between chunks.
    pub fn upload(&self, anim: &Animation, layer: u8, progress: &AtomicUsize, cancel: &AtomicBool) -> Result<()> {
        let count = anim.frames.len();
        if count == 0 || count > MAX_FRAMES {
            return Err(anyhow!("{count} frames (1..={MAX_FRAMES} supported)"));
        }
        let still = count == 1;
        let (first, delay) = if still { (layer, 0) } else { (0, anim.delay_ms) };
        self.begin(first, count as u8, delay)?;
        sleep_ms(100);
        for (i, px) in anim.frames.iter().enumerate() {
            let frame = if still { layer } else { i as u8 };
            for chunk in data_payloads(frame, count as u8, delay, px) {
                if cancel.load(Ordering::Relaxed) {
                    return Err(anyhow!("upload cancelled"));
                }
                sleep_ms(3);
                self.chan.write_as(&chunk, Checksum::Bit7, Target::Direct)?;
                progress.fetch_add(1, Ordering::Relaxed);
            }
        }
        sleep_ms(500);
        Ok(())
    }

    /// A5 handshake: resend every 100 ms until the receiver reports ready.
    fn begin(&self, first: u8, count: u8, delay: u8) -> Result<()> {
        let payload = begin_payload(first, count, delay);
        for _ in 0..=10 {
            self.chan.write_as(&payload, Checksum::Bit7, Target::Direct)?;
            sleep_ms(100);
            let reply = self.chan.read();
            if std::env::var_os("PERICORE_DEBUG").is_some() {
                println!("   A5 reply: {:02X?}", reply.as_ref().map(|r| &r[..12]));
            }
            if reply.is_ok_and(|r| r[1] == 1) {
                return Ok(());
            }
            sleep_ms(100);
        }
        Err(anyhow!("the dock screen didn't get ready (is the AJAZZ app closed?)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::Checksum;

    #[test]
    fn begin_matches_spec_full_screen() {
        let p = Checksum::Bit7.apply(&begin_payload(0, 2, 100));
        assert_eq!(&p[..18], &[0xA5, 0, 2, 0x64, 0x00, 0x80, 0, 0x74, 0, 0, 0x80, 0x80, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn data_chunks_match_spec() {
        let px = vec![0u8; FRAME_BYTES];
        let d = data_payloads(0, 2, 100, &px);
        assert_eq!(d.len(), 586);
        assert_eq!(&Checksum::Bit7.apply(&d[0])[..8], &[0x25, 0, 2, 0x64, 0, 0, 0x38, 0x3C]);
        assert_eq!(&Checksum::Bit7.apply(&d[585])[..8], &[0x25, 0, 2, 0x64, 0x49, 0x02, 0x08, 0x21]);
        let d1 = data_payloads(1, 2, 100, &px);
        assert_eq!(Checksum::Bit7.apply(&d1[0])[7], 0x3B);
    }

    #[test]
    fn time_sync_matches_spec() {
        let p = Checksum::Bit7.apply(&time_payload(2026, 11, 18, 14, 30, 5));
        assert_eq!(&p[..15], &[0x28, 0, 0, 0, 0, 0, 0, 0xD7, 0x07, 0xEA, 0x0B, 0x12, 0x0E, 0x1E, 0x05]);
    }

    #[test]
    fn rgb565_is_big_endian_column_major() {
        let mut img = RgbaImage::from_pixel(SIZE, SIZE, Rgba([0, 0, 0, 255]));
        img.put_pixel(0, 1, Rgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([0, 0, 255, 255]));
        let v = to_rgb565_columns(&img);
        assert_eq!(&v[2..4], &[0xF8, 0x00]); // (x0,y1) is 2nd pixel
        assert_eq!(&v[256..258], &[0x00, 0x1F]); // (x1,y0) starts column 1
    }
}
