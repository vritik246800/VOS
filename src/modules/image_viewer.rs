use std::{
    fs::File,
    io::BufReader,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use image::{AnimationDecoder, DynamicImage, codecs::gif::GifDecoder};

use super::raster::{RasterImage, ViewTransform};

const LARGEST_PICTURE: u32 = 1600;
const DEFAULT_GIF_DELAY: Duration = Duration::from_millis(100);

struct ImageFrame {
    image: RasterImage,
    delay: Duration,
}

pub struct ImageViewer {
    pub title: String,
    frames: Vec<ImageFrame>,
    frame: usize,
    next_frame: Instant,
    playing: bool,
    pub view: ViewTransform,
    cache: Option<((u32, u32, u64), RasterImage)>,
    version: u64,
}

impl ImageViewer {
    pub fn open(path: &Path) -> Result<Self> {
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image")
            .to_owned();
        let is_gif = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"));
        let frames = if is_gif {
            gif_frames(path)?
        } else {
            let image = image::ImageReader::open(path)
                .with_context(|| format!("Could not open {}", path.display()))?
                .with_guessed_format()?
                .decode()
                .with_context(|| format!("Could not decode {}", path.display()))?;
            vec![ImageFrame {
                image: RasterImage::from_dynamic(fit(image)),
                delay: DEFAULT_GIF_DELAY,
            }]
        };
        if frames.is_empty() {
            bail!("{} has no image frames", path.display());
        }
        Ok(Self {
            title,
            next_frame: Instant::now() + frames[0].delay,
            playing: frames.len() > 1,
            frames,
            frame: 0,
            view: ViewTransform::default(),
            cache: None,
            version: 0,
        })
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn is_animated(&self) -> bool {
        self.frames.len() > 1
    }

    pub fn needs_tick(&self) -> bool {
        self.is_animated() && self.playing
    }

    pub fn tick(&mut self, now: Instant) -> bool {
        if !self.needs_tick() || now < self.next_frame {
            return false;
        }
        self.frame = (self.frame + 1) % self.frames.len();
        self.next_frame = now + self.frames[self.frame].delay;
        self.changed();
        true
    }

    pub fn toggle_playback(&mut self) {
        if self.is_animated() {
            self.playing = !self.playing;
            self.next_frame = Instant::now() + self.frames[self.frame].delay;
            self.changed();
        }
    }

    pub fn step_frame(&mut self, forward: bool) {
        if !self.is_animated() {
            return;
        }
        self.playing = false;
        self.frame = if forward {
            (self.frame + 1) % self.frames.len()
        } else {
            (self.frame + self.frames.len() - 1) % self.frames.len()
        };
        self.next_frame = Instant::now() + self.frames[self.frame].delay;
        self.changed();
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.view.zoom_by(factor);
        self.changed();
    }

    pub fn pan_by(&mut self, x: f32, y: f32) {
        let before = self.view;
        self.view.pan_by(x, y);
        if self.view != before {
            self.changed();
        }
    }

    pub fn reset_view(&mut self) {
        self.view.reset();
        self.changed();
    }

    pub fn status(&self) -> String {
        let mut status = format!("Zoom {}%", (self.view.zoom * 100.0).round() as u16);
        if self.is_animated() {
            status.push_str(&format!(
                "  Frame {}/{}  {}",
                self.frame + 1,
                self.frames.len(),
                if self.playing { "Playing" } else { "Paused" }
            ));
        }
        status
    }

    pub fn render(&mut self, width: u32, height: u32) -> &RasterImage {
        let key = (width, height, self.version);
        if self.cache.as_ref().is_none_or(|(cached, _)| *cached != key) {
            let image = self.frames[self.frame]
                .image
                .render(width, height, self.view);
            self.cache = Some((key, image));
        }
        &self.cache.as_ref().unwrap().1
    }

    fn changed(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.cache = None;
    }
}

fn fit(image: DynamicImage) -> DynamicImage {
    if image.width().max(image.height()) > LARGEST_PICTURE {
        image.thumbnail(LARGEST_PICTURE, LARGEST_PICTURE)
    } else {
        image
    }
}

fn gif_frames(path: &Path) -> Result<Vec<ImageFrame>> {
    let file = File::open(path).with_context(|| format!("Could not open {}", path.display()))?;
    let frames = GifDecoder::new(BufReader::new(file))
        .and_then(|decoder| decoder.into_frames().collect_frames())
        .with_context(|| format!("Could not decode {}", path.display()))?;
    Ok(frames
        .into_iter()
        .map(|frame| {
            let (numerator, denominator) = frame.delay().numer_denom_ms();
            let delay = Duration::from_millis(u64::from(numerator / denominator.max(1)));
            ImageFrame {
                image: RasterImage::from_dynamic(fit(DynamicImage::ImageRgba8(
                    frame.into_buffer(),
                ))),
                delay: if delay.is_zero() {
                    DEFAULT_GIF_DELAY
                } else {
                    delay
                },
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use image::{Delay, Frame, Rgba, RgbaImage, codecs::gif::GifEncoder};
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn animated_gif_opens_and_advances_on_time() {
        let directory =
            PathBuf::from("target").join(format!("vos-gif-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("blink.gif");
        let mut encoder = GifEncoder::new(File::create(&path).unwrap());
        for (colour, milliseconds) in [([255, 0, 0, 255], 200), ([0, 0, 255, 255], 0)] {
            encoder
                .encode_frame(Frame::from_parts(
                    RgbaImage::from_pixel(4, 4, Rgba(colour)),
                    0,
                    0,
                    Delay::from_numer_denom_ms(milliseconds, 1),
                ))
                .unwrap();
        }
        drop(encoder);

        let mut viewer = ImageViewer::open(&path).unwrap();
        assert!(viewer.is_animated() && viewer.needs_tick());
        let first = viewer.render(4, 2).clone();
        let now = Instant::now();
        assert!(!viewer.tick(now));
        assert!(viewer.tick(now + Duration::from_millis(250)));
        assert_ne!(viewer.render(4, 2).rgb(), first.rgb());
        assert_eq!(viewer.frames[1].delay, DEFAULT_GIF_DELAY);
        viewer.toggle_playback();
        assert!(!viewer.needs_tick());
        viewer.step_frame(false);
        assert!(viewer.status().contains("Frame 1/2"));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
