use image::{DynamicImage, Rgb, RgbImage, imageops};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewTransform {
    pub zoom: f32,
    pub pan_x: f32,
    pub pan_y: f32,
}

impl Default for ViewTransform {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            pan_x: 0.0,
            pan_y: 0.0,
        }
    }
}

impl ViewTransform {
    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom = (self.zoom * factor).clamp(0.25, 8.0);
        if self.zoom <= 1.0 {
            self.pan_x = 0.0;
            self.pan_y = 0.0;
        }
    }

    pub fn pan_by(&mut self, x: f32, y: f32) {
        if self.zoom > 1.0 {
            self.pan_x = (self.pan_x + x).clamp(-1.0, 1.0);
            self.pan_y = (self.pan_y + y).clamp(-1.0, 1.0);
        }
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[derive(Clone, Debug)]
pub struct RasterImage {
    rgb: Vec<u8>,
    width: u32,
    height: u32,
}

impl RasterImage {
    pub fn from_rgb(width: u32, height: u32, rgb: Vec<u8>) -> Option<Self> {
        RgbImage::from_raw(width, height, rgb).map(|image| Self {
            rgb: image.into_raw(),
            width,
            height,
        })
    }

    pub fn from_dynamic(image: DynamicImage) -> Self {
        let image = image.to_rgb8();
        Self {
            width: image.width(),
            height: image.height(),
            rgb: image.into_raw(),
        }
    }

    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    pub fn rgb(&self) -> &[u8] {
        &self.rgb
    }

    pub fn into_rgb(self) -> Vec<u8> {
        self.rgb
    }

    pub fn render(&self, viewport_width: u32, viewport_height: u32, view: ViewTransform) -> Self {
        if viewport_width == 0 || viewport_height == 0 {
            return Self::from_rgb(1, 1, vec![0, 0, 0]).unwrap();
        }
        let Some(source) = RgbImage::from_raw(self.width, self.height, self.rgb.clone()) else {
            return Self::from_rgb(1, 1, vec![0, 0, 0]).unwrap();
        };

        let fit = (viewport_width as f64 / self.width.max(1) as f64)
            .min(viewport_height as f64 / self.height.max(1) as f64);
        let scale = fit * f64::from(view.zoom);
        let scaled_width = ((self.width as f64 * scale).round() as i64).max(1);
        let scaled_height = ((self.height as f64 * scale).round() as i64).max(1);
        let viewport_width_i = i64::from(viewport_width);
        let viewport_height_i = i64::from(viewport_height);

        let left = centered_offset(scaled_width, viewport_width_i, view.pan_x);
        let top = centered_offset(scaled_height, viewport_height_i, view.pan_y);
        let x0 = left.max(0);
        let y0 = top.max(0);
        let x1 = (left + scaled_width).min(viewport_width_i);
        let y1 = (top + scaled_height).min(viewport_height_i);

        let mut canvas = RgbImage::from_pixel(viewport_width, viewport_height, Rgb([0, 0, 0]));
        if x0 >= x1 || y0 >= y1 {
            return Self::from_dynamic(DynamicImage::ImageRgb8(canvas));
        }

        let scale_x = scaled_width as f64 / self.width.max(1) as f64;
        let scale_y = scaled_height as f64 / self.height.max(1) as f64;
        let crop_x = (((x0 - left) as f64 / scale_x).floor() as u32).min(self.width - 1);
        let crop_y = (((y0 - top) as f64 / scale_y).floor() as u32).min(self.height - 1);
        let crop_right = (((x1 - left) as f64 / scale_x).ceil() as u32).min(self.width);
        let crop_bottom = (((y1 - top) as f64 / scale_y).ceil() as u32).min(self.height);
        let crop = imageops::crop_imm(
            &source,
            crop_x,
            crop_y,
            (crop_right - crop_x).max(1),
            (crop_bottom - crop_y).max(1),
        )
        .to_image();
        let resized = imageops::resize(
            &crop,
            (x1 - x0) as u32,
            (y1 - y0) as u32,
            imageops::FilterType::Triangle,
        );
        imageops::overlay(&mut canvas, &resized, x0, y0);
        Self::from_dynamic(DynamicImage::ImageRgb8(canvas))
    }
}

fn centered_offset(scaled: i64, viewport: i64, pan: f32) -> i64 {
    let centered = (viewport - scaled) / 2;
    if scaled <= viewport {
        centered
    } else {
        let max_offset = (scaled - viewport) / 2;
        let requested = (pan * viewport as f32).round() as i64;
        (centered + requested)
            .clamp(viewport - scaled, 0)
            .min(max_offset)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, colour: [u8; 3]) -> RasterImage {
        RasterImage::from_dynamic(DynamicImage::ImageRgb8(RgbImage::from_pixel(
            width,
            height,
            Rgb(colour),
        )))
    }

    #[test]
    fn fit_keeps_proportions_and_centres_on_black() {
        let rendered = solid(4, 2, [255, 0, 0]).render(8, 8, ViewTransform::default());
        let image = RgbImage::from_raw(8, 8, rendered.into_rgb()).unwrap();
        assert_eq!(image.get_pixel(0, 0).0, [0, 0, 0]);
        assert_eq!(image.get_pixel(0, 1).0, [0, 0, 0]);
        assert_eq!(image.get_pixel(0, 2).0, [255, 0, 0]);
        assert_eq!(image.get_pixel(7, 5).0, [255, 0, 0]);
        assert_eq!(image.get_pixel(0, 6).0, [0, 0, 0]);
    }

    #[test]
    fn zoom_pan_and_reset_are_bounded() {
        let mut view = ViewTransform::default();
        view.pan_by(1.0, 1.0);
        assert_eq!(view.pan_x, 0.0, "a fitted image cannot be panned");
        view.zoom_by(2.0);
        view.pan_by(2.0, -2.0);
        assert_eq!((view.pan_x, view.pan_y), (1.0, -1.0));
        for _ in 0..20 {
            view.zoom_by(2.0);
        }
        assert_eq!(view.zoom, 8.0);
        view.reset();
        assert_eq!(view, ViewTransform::default());
    }

    #[test]
    fn zoom_renders_only_the_visible_crop() {
        let rendered = solid(4, 2, [0, 255, 0]).render(
            8,
            8,
            ViewTransform {
                zoom: 2.0,
                pan_x: 1.0,
                pan_y: 0.0,
            },
        );
        assert_eq!(rendered.dimensions(), (8, 8));
        assert!(
            rendered
                .into_rgb()
                .chunks_exact(3)
                .any(|p| p == [0, 255, 0])
        );
        assert_eq!(
            solid(2, 2, [0, 0, 0])
                .render(0, 0, ViewTransform::default())
                .dimensions(),
            (1, 1)
        );
    }
}
