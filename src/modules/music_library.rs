use ratatui::widgets::ListState;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "flac", "wav", "ogg", "aac", "m4a", "opus", "wma", "aiff",
];

#[derive(Debug, Clone)]
pub struct MusicTrack {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_secs: f64,
}

impl MusicTrack {
    pub fn fmt_duration(&self) -> String {
        let t = self.duration_secs as u64;
        format!("{}:{:02}", t / 60, t % 60)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum LibraryView {
    Artists,
    Albums,
    Tracks,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LibraryFocus {
    Library,
    Queue,
}

pub struct MusicLibrary {
    pub view: LibraryView,
    pub focus: LibraryFocus,
    pub all_tracks: Vec<MusicTrack>,
    pub artists: Vec<String>,
    pub artist_state: ListState,
    pub selected_artist: Option<String>,
    pub albums: Vec<String>,
    pub album_state: ListState,
    pub selected_album: Option<String>,
    pub tracks: Vec<MusicTrack>,
    pub track_state: ListState,
    pub queue_state: ListState,
    pub scanning: bool,
    pub scan_count: usize,
}

impl MusicLibrary {
    pub fn new() -> Self {
        let mut artist_state = ListState::default();
        artist_state.select(Some(0));
        Self {
            view: LibraryView::Artists,
            focus: LibraryFocus::Library,
            all_tracks: Vec::new(),
            artists: Vec::new(),
            artist_state,
            selected_artist: None,
            albums: Vec::new(),
            album_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            selected_album: None,
            tracks: Vec::new(),
            track_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            queue_state: {
                let mut s = ListState::default();
                s.select(Some(0));
                s
            },
            scanning: false,
            scan_count: 0,
        }
    }

    pub fn scan_directory(&mut self, dir: &Path) {
        self.scanning = true;
        self.all_tracks.clear();
        self.scan_count = 0;
        self.all_tracks = scan_music_dir(dir);
        self.scan_count = self.all_tracks.len();
        self.rebuild_artists();
        self.scanning = false;
    }

    pub fn load_tracks(&mut self, tracks: Vec<MusicTrack>) {
        self.all_tracks = tracks;
        self.rebuild_artists();
    }

    pub fn rebuild_artists(&mut self) {
        let artists: BTreeSet<String> = self
            .all_tracks
            .iter()
            .map(|t| {
                if t.artist.is_empty() {
                    "Unknown Artist".to_string()
                } else {
                    t.artist.clone()
                }
            })
            .collect();
        self.artists = artists.into_iter().collect();
        if !self.artists.is_empty() {
            self.artist_state.select(Some(0));
        }
    }

    pub fn enter(&mut self) {
        match self.view {
            LibraryView::Artists => self.enter_artist(),
            LibraryView::Albums => self.enter_album(),
            LibraryView::Tracks => {} // handled externally (play)
        }
    }

    fn enter_artist(&mut self) {
        let Some(idx) = self.artist_state.selected() else {
            return;
        };
        let Some(artist) = self.artists.get(idx).cloned() else {
            return;
        };
        let albums: BTreeSet<String> = self
            .all_tracks
            .iter()
            .filter(|t| effective_artist(t) == artist)
            .map(|t| effective_album(t))
            .collect();
        self.albums = albums.into_iter().collect();
        self.selected_artist = Some(artist);
        self.album_state.select(Some(0));
        self.view = LibraryView::Albums;
    }

    fn enter_album(&mut self) {
        let Some(idx) = self.album_state.selected() else {
            return;
        };
        let Some(album) = self.albums.get(idx).cloned() else {
            return;
        };
        let artist = self.selected_artist.clone().unwrap_or_default();
        let mut tracks: Vec<MusicTrack> = self
            .all_tracks
            .iter()
            .filter(|t| effective_artist(t) == artist && effective_album(t) == album)
            .cloned()
            .collect();
        tracks.sort_by(|a, b| a.title.cmp(&b.title));
        self.selected_album = Some(album);
        self.tracks = tracks;
        self.track_state.select(Some(0));
        self.view = LibraryView::Tracks;
    }

    pub fn go_back(&mut self) {
        match self.view {
            LibraryView::Albums => {
                self.view = LibraryView::Artists;
                self.selected_artist = None;
            }
            LibraryView::Tracks => {
                self.view = LibraryView::Albums;
                self.selected_album = None;
            }
            LibraryView::Artists => {}
        }
    }

    pub fn selected_track(&self) -> Option<&MusicTrack> {
        self.track_state.selected().and_then(|i| self.tracks.get(i))
    }

    pub fn nav_up(&mut self) {
        match self.view {
            LibraryView::Artists => nav_list_up(&mut self.artist_state, self.artists.len()),
            LibraryView::Albums => nav_list_up(&mut self.album_state, self.albums.len()),
            LibraryView::Tracks => nav_list_up(&mut self.track_state, self.tracks.len()),
        }
    }

    pub fn nav_down(&mut self) {
        match self.view {
            LibraryView::Artists => nav_list_down(&mut self.artist_state, self.artists.len()),
            LibraryView::Albums => nav_list_down(&mut self.album_state, self.albums.len()),
            LibraryView::Tracks => nav_list_down(&mut self.track_state, self.tracks.len()),
        }
    }

    pub fn queue_up(&mut self, len: usize) {
        nav_list_up(&mut self.queue_state, len);
    }
    pub fn queue_down(&mut self, len: usize) {
        nav_list_down(&mut self.queue_state, len);
    }
}

fn effective_artist(t: &MusicTrack) -> String {
    if t.artist.is_empty() {
        "Unknown Artist".to_string()
    } else {
        t.artist.clone()
    }
}
fn effective_album(t: &MusicTrack) -> String {
    if t.album.is_empty() {
        "Unknown Album".to_string()
    } else {
        t.album.clone()
    }
}

fn nav_list_up(state: &mut ListState, len: usize) {
    if len == 0 {
        return;
    }
    let i = state.selected().unwrap_or(0);
    state.select(Some(if i == 0 { len - 1 } else { i - 1 }));
}
fn nav_list_down(state: &mut ListState, len: usize) {
    if len == 0 {
        return;
    }
    let i = state.selected().unwrap_or(0);
    state.select(Some((i + 1) % len));
}

pub fn scan_music_dir(dir: &Path) -> Vec<MusicTrack> {
    let mut tracks = Vec::new();
    scan_recursive(dir, &mut tracks);
    tracks.sort_by(|a, b| {
        a.artist
            .cmp(&b.artist)
            .then(a.album.cmp(&b.album))
            .then(a.title.cmp(&b.title))
    });
    tracks
}

fn scan_recursive(dir: &Path, out: &mut Vec<MusicTrack>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_recursive(&path, out);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if AUDIO_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                out.push(read_track_meta(&path));
            }
        }
    }
}

pub fn read_track_meta(path: &Path) -> MusicTrack {
    let fallback_title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    match try_read_lofty_meta(path) {
        Some((title, artist, album, dur)) => MusicTrack {
            path: path.to_path_buf(),
            title,
            artist,
            album,
            duration_secs: dur,
        },
        None => MusicTrack {
            path: path.to_path_buf(),
            title: fallback_title,
            artist: String::new(),
            album: String::new(),
            duration_secs: 0.0,
        },
    }
}

fn try_read_lofty_meta(path: &Path) -> Option<(String, String, String, f64)> {
    use lofty::prelude::*;

    let tagged = lofty::read_from_path(path).ok()?;
    let duration = tagged.properties().duration().as_secs_f64();
    let fallback = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Unknown")
        .to_string();

    if let Some(tag) = tagged.primary_tag() {
        let title = tag.title().map(|t| t.to_string()).unwrap_or(fallback);
        let artist = tag.artist().map(|a| a.to_string()).unwrap_or_default();
        let album = tag.album().map(|a| a.to_string()).unwrap_or_default();
        Some((title, artist, album, duration))
    } else {
        Some((fallback, String::new(), String::new(), duration))
    }
}

pub fn read_cover_art(path: &Path) -> Option<(Vec<u8>, u32, u32)> {
    use lofty::prelude::*;

    let tagged = lofty::read_from_path(path).ok()?;
    let tag = tagged.primary_tag()?;
    let picture = tag.pictures().first()?;
    let img = image::load_from_memory(picture.data()).ok()?;
    // Resize to max 300x300 for cover art
    let img = img.resize(300, 300, image::imageops::FilterType::Nearest);
    let w = img.width();
    let h = img.height();
    Some((img.to_rgb8().into_raw(), w, h))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fmt_duration() {
        let t = MusicTrack {
            path: PathBuf::new(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            duration_secs: 185.0,
        };
        assert_eq!(t.fmt_duration(), "3:05");
    }

    #[test]
    fn test_nav_wraps() {
        let mut state = ListState::default();
        state.select(Some(0));
        nav_list_up(&mut state, 3);
        assert_eq!(state.selected(), Some(2));
    }

    #[test]
    fn test_nav_down() {
        let mut state = ListState::default();
        state.select(Some(2));
        nav_list_down(&mut state, 3);
        assert_eq!(state.selected(), Some(0));
    }

    #[test]
    fn test_library_build_artists() {
        let mut lib = MusicLibrary::new();
        let tracks = vec![
            MusicTrack {
                path: PathBuf::from("a.mp3"),
                title: "A".into(),
                artist: "Art1".into(),
                album: "Alb1".into(),
                duration_secs: 1.0,
            },
            MusicTrack {
                path: PathBuf::from("b.mp3"),
                title: "B".into(),
                artist: "Art2".into(),
                album: "Alb2".into(),
                duration_secs: 2.0,
            },
            MusicTrack {
                path: PathBuf::from("c.mp3"),
                title: "C".into(),
                artist: "Art1".into(),
                album: "Alb1".into(),
                duration_secs: 3.0,
            },
        ];
        lib.load_tracks(tracks);
        assert_eq!(lib.artists.len(), 2);
        assert_eq!(lib.artists[0], "Art1");
        assert_eq!(lib.artists[1], "Art2");
    }
}
