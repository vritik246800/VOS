use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, anyhow, bail};

use super::raster::{RasterImage, ViewTransform};

const PAGE_PIXELS: u32 = 1200;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfMode {
    Visual,
    Text,
}

pub struct PdfDocument {
    path: PathBuf,
    pub title: String,
    texts: Vec<Vec<String>>,
    rendered: HashMap<usize, RasterImage>,
    pub page: usize,
    pub mode: PdfMode,
    pub scroll: usize,
    pub view: ViewTransform,
    cache: Option<((u32, u32, u64), RasterImage)>,
    version: u64,
}

impl PdfDocument {
    pub fn open(path: &Path) -> Result<Self> {
        let page_count = page_count(path)?;
        let texts = page_texts(path, page_count)?;
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("PDF")
            .to_owned();
        let mut document = Self {
            path: path.to_path_buf(),
            title,
            texts,
            rendered: HashMap::new(),
            page: 0,
            mode: PdfMode::Visual,
            scroll: 0,
            view: ViewTransform::default(),
            cache: None,
            version: 0,
        };
        document.load_page(0)?;
        Ok(document)
    }

    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn pages(&self) -> usize {
        self.texts.len()
    }

    pub fn is_visual(&self) -> bool {
        self.mode == PdfMode::Visual
    }

    pub fn show_page(&mut self, page: usize) -> Result<()> {
        let page = page.min(self.pages().saturating_sub(1));
        if page == self.page {
            return Ok(());
        }
        self.load_page(page)?;
        self.page = page;
        self.scroll = 0;
        self.view.reset();
        self.rendered.retain(|cached, _| cached.abs_diff(page) <= 2);
        self.changed();
        Ok(())
    }

    pub fn next_page(&mut self) -> Result<()> {
        self.show_page(self.page.saturating_add(1))
    }

    pub fn previous_page(&mut self) -> Result<()> {
        self.show_page(self.page.saturating_sub(1))
    }

    pub fn toggle_mode(&mut self) -> Result<()> {
        self.mode = match self.mode {
            PdfMode::Visual => PdfMode::Text,
            PdfMode::Text => PdfMode::Visual,
        };
        self.scroll = 0;
        self.view.reset();
        self.load_page(self.page)?;
        self.changed();
        Ok(())
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

    pub fn current_lines(&self) -> &[String] {
        &self.texts[self.page]
    }

    pub fn max_scroll(&self, visible_height: usize) -> usize {
        self.current_lines()
            .len()
            .saturating_sub(visible_height.max(1))
    }

    pub fn scroll_by(&mut self, amount: isize, visible_height: usize) {
        let max = self.max_scroll(visible_height);
        self.scroll = if amount.is_negative() {
            self.scroll.saturating_sub(amount.unsigned_abs())
        } else {
            self.scroll.saturating_add(amount as usize).min(max)
        };
    }

    pub fn render(&mut self, width: u32, height: u32) -> Option<&RasterImage> {
        if !self.is_visual() {
            return None;
        }
        let key = (width, height, self.version);
        if self.cache.as_ref().is_none_or(|(cached, _)| *cached != key) {
            let image = self
                .rendered
                .get(&self.page)?
                .render(width, height, self.view);
            self.cache = Some((key, image));
        }
        Some(&self.cache.as_ref().unwrap().1)
    }

    fn load_page(&mut self, page: usize) -> Result<()> {
        if self.rendered.contains_key(&page) {
            return Ok(());
        }
        let image = render_page(&self.path, page)?;
        self.rendered.insert(page, image);
        Ok(())
    }

    fn changed(&mut self) {
        self.version = self.version.wrapping_add(1);
        self.cache = None;
    }
}

fn page_count(path: &Path) -> Result<usize> {
    let output = Command::new("pdfinfo")
        .arg(path)
        .output()
        .context("pdfinfo was not found; install poppler")?;
    if !output.status.success() {
        bail!("Could not read {} as a PDF", path.display());
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("Pages:")?.trim().parse::<usize>().ok())
        .filter(|pages| *pages > 0)
        .ok_or_else(|| anyhow!("The PDF has no pages"))
}

fn page_texts(path: &Path, page_count: usize) -> Result<Vec<Vec<String>>> {
    let output = Command::new("pdftotext")
        .arg("-layout")
        .arg(path)
        .arg("-")
        .output()
        .context("pdftotext was not found; install poppler")?;
    if !output.status.success() {
        bail!("Could not extract text from {}", path.display());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut pages = text
        .split('\u{000c}')
        .map(|page| {
            page.lines()
                .map(str::trim_end)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    pages.resize_with(page_count, Vec::new);
    pages.truncate(page_count);
    Ok(pages)
}

fn render_page(path: &Path, page: usize) -> Result<RasterImage> {
    let output = Command::new("pdftoppm")
        .args([
            "-f",
            &(page + 1).to_string(),
            "-l",
            &(page + 1).to_string(),
            "-singlefile",
            "-scale-to-x",
            &PAGE_PIXELS.to_string(),
            "-scale-to-y",
            "-1",
        ])
        .arg(path)
        .output()
        .context("pdftoppm was not found; install poppler")?;
    if !output.status.success() {
        bail!(
            "Could not draw page {} of {}: {}",
            page + 1,
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    parse_ppm(&output.stdout)
}

fn parse_ppm(bytes: &[u8]) -> Result<RasterImage> {
    let mut cursor = 0;
    let magic = ppm_token(bytes, &mut cursor)?;
    if magic != b"P6" {
        bail!("Unexpected PDF image format");
    }
    let width = ppm_number(bytes, &mut cursor)?;
    let height = ppm_number(bytes, &mut cursor)?;
    let maximum = ppm_number(bytes, &mut cursor)?;
    if maximum != 255 {
        bail!("Unsupported PDF image colour depth");
    }
    if cursor >= bytes.len() || !bytes[cursor].is_ascii_whitespace() {
        bail!("Invalid PDF image header");
    }
    cursor += 1;
    if cursor < bytes.len() && bytes[cursor - 1] == b'\r' && bytes[cursor] == b'\n' {
        cursor += 1;
    }
    let length = width as usize * height as usize * 3;
    let pixels = bytes
        .get(cursor..cursor + length)
        .ok_or_else(|| anyhow!("Incomplete PDF image"))?
        .to_vec();
    RasterImage::from_rgb(width, height, pixels).ok_or_else(|| anyhow!("Invalid PDF image"))
}

fn ppm_number(bytes: &[u8], cursor: &mut usize) -> Result<u32> {
    String::from_utf8(ppm_token(bytes, cursor)?.to_vec())?
        .parse::<u32>()
        .map_err(Into::into)
}

fn ppm_token<'a>(bytes: &'a [u8], cursor: &mut usize) -> Result<&'a [u8]> {
    loop {
        while bytes.get(*cursor).is_some_and(u8::is_ascii_whitespace) {
            *cursor += 1;
        }
        if bytes.get(*cursor) == Some(&b'#') {
            while bytes.get(*cursor).is_some_and(|byte| *byte != b'\n') {
                *cursor += 1;
            }
        } else {
            break;
        }
    }
    let start = *cursor;
    while bytes
        .get(*cursor)
        .is_some_and(|byte| !byte.is_ascii_whitespace())
    {
        *cursor += 1;
    }
    if start == *cursor {
        bail!("Incomplete PDF image header");
    }
    Ok(&bytes[start..*cursor])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ppm_pages_keep_their_rgb_pixels() {
        let bytes = b"P6\n# rendered page\n2 1\n255\n\x01\x02\x03\x04\x05\x06";
        let image = parse_ppm(bytes).unwrap();
        assert_eq!(image.dimensions(), (2, 1));
        assert_eq!(image.rgb(), &[1, 2, 3, 4, 5, 6]);
        assert!(parse_ppm(b"P6\n2 1\n255\nshort").is_err());
        assert!(parse_ppm(b"P3\n1 1\n255\n0 0 0").is_err());
    }

    #[test]
    fn text_scrolling_stops_at_the_page_end() {
        let mut document = PdfDocument {
            path: PathBuf::new(),
            title: String::new(),
            texts: vec![(0..10).map(|line| line.to_string()).collect()],
            rendered: HashMap::new(),
            page: 0,
            mode: PdfMode::Text,
            scroll: 0,
            view: ViewTransform::default(),
            cache: None,
            version: 0,
        };
        document.scroll_by(20, 4);
        assert_eq!(document.scroll, 6);
        document.scroll_by(-2, 4);
        assert_eq!(document.scroll, 4);
        assert_eq!(document.max_scroll(20), 0);
    }

    #[test]
    fn poppler_opens_pages_and_their_text_when_available() {
        if ["pdfinfo", "pdftotext", "pdftoppm"]
            .iter()
            .any(|tool| Command::new(tool).arg("-v").output().is_err())
        {
            return;
        }
        let directory =
            PathBuf::from("target").join(format!("vos-pdf-test-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("sample.pdf");
        std::fs::write(&path, sample_pdf()).unwrap();

        let mut document = PdfDocument::open(&path).unwrap();
        assert_eq!(document.pages(), 2);
        assert!(document.is_visual());
        let page = document.render(80, 40).unwrap();
        assert!(page.rgb().chunks_exact(3).any(|pixel| pixel != [0, 0, 0]));
        document.next_page().unwrap();
        assert_eq!(document.page, 1);
        document.toggle_mode().unwrap();
        assert!(!document.is_visual());
        assert!(
            document
                .current_lines()
                .iter()
                .any(|line| line.contains("Second page"))
        );
        std::fs::remove_dir_all(directory).unwrap();
    }

    fn sample_pdf() -> Vec<u8> {
        let contents = [
            "BT /F1 10 Tf 5 80 Td (First page) Tj ET",
            "BT /F1 10 Tf 5 80 Td (Second page) Tj ET 0 0 1 rg 20 20 60 60 re f",
        ];
        let mut objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_owned(),
        ];
        let font = "<< /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >>";
        for (page, content) in contents.iter().enumerate() {
            objects.push(format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] /Resources {font} /Contents {} 0 R >>",
                4 + page * 2
            ));
            objects.push(format!(
                "<< /Length {} >>\nstream\n{content}\nendstream",
                content.len()
            ));
        }
        let mut pdf = String::from("%PDF-1.4\n");
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
        }
        let xref = pdf.len();
        pdf.push_str("xref\n0 7\n0000000000 65535 f \n");
        for offset in offsets {
            pdf.push_str(&format!("{offset:010} 00000 n \n"));
        }
        pdf.push_str(&format!(
            "trailer\n<< /Size 7 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n"
        ));
        pdf.into_bytes()
    }
}
