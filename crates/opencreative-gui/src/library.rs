//! The kinds of media the Multimedia page offers, each with its own
//! playlist: still images, GIFs, video files and web videos (YouTube).

use std::path::Path;

use opencreative_config::MediaItem;
use serde::{Deserialize, Serialize};

use crate::DEFAULT_ITEM_SECS;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    Image,
    Gif,
    Video,
    Youtube,
}

/// Still images the file picker offers (GIFs have their own tab).
pub const IMAGE_EXTENSIONS: [&str; 5] = ["png", "jpg", "jpeg", "webp", "bmp"];

impl MediaKind {
    pub const ALL: [MediaKind; 4] = [
        MediaKind::Image,
        MediaKind::Gif,
        MediaKind::Video,
        MediaKind::Youtube,
    ];

    pub fn of(item: &MediaItem) -> Self {
        match &item.path {
            Some(path) => Self::of_path(path),
            None => MediaKind::Youtube,
        }
    }

    pub fn of_path(path: &Path) -> Self {
        let gif = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gif"));
        if gif {
            MediaKind::Gif
        } else if opencreative_render::media::is_video_path(path) {
            MediaKind::Video
        } else {
            MediaKind::Image
        }
    }

    /// Tab name.
    pub fn name(self) -> &'static str {
        match self {
            MediaKind::Image => "Images",
            MediaKind::Gif => "GIFs",
            MediaKind::Video => "Videos",
            MediaKind::Youtube => "YouTube",
        }
    }

    /// Lower-case noun for sentences ("Added 3 images").
    pub fn noun(self, count: usize) -> &'static str {
        match (self, count) {
            (MediaKind::Image, 1) => "image",
            (MediaKind::Image, _) => "images",
            (MediaKind::Gif, 1) => "GIF",
            (MediaKind::Gif, _) => "GIFs",
            (MediaKind::Video, 1) => "video",
            (MediaKind::Video, _) => "videos",
            (MediaKind::Youtube, 1) => "link",
            (MediaKind::Youtube, _) => "links",
        }
    }

    /// File extensions the picker offers for this kind (none for web links).
    pub fn extensions(self) -> &'static [&'static str] {
        match self {
            MediaKind::Image => &IMAGE_EXTENSIONS,
            MediaKind::Gif => &["gif"],
            MediaKind::Video => &opencreative_render::media::VIDEO_EXTENSIONS,
            MediaKind::Youtube => &[],
        }
    }
}

/// One kind's playlist and how long each item shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Playlist {
    pub items: Vec<MediaItem>,
    pub item_secs: f64,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            item_secs: DEFAULT_ITEM_SECS,
        }
    }
}

/// Every kind's playlist, saved with the app's preferences so switching
/// tabs (or restarting) keeps them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Playlists {
    pub image: Playlist,
    pub gif: Playlist,
    pub video: Playlist,
    pub youtube: Playlist,
}

impl Playlists {
    pub fn get(&self, kind: MediaKind) -> &Playlist {
        match kind {
            MediaKind::Image => &self.image,
            MediaKind::Gif => &self.gif,
            MediaKind::Video => &self.video,
            MediaKind::Youtube => &self.youtube,
        }
    }

    pub fn get_mut(&mut self, kind: MediaKind) -> &mut Playlist {
        match kind {
            MediaKind::Image => &mut self.image,
            MediaKind::Gif => &mut self.gif,
            MediaKind::Video => &mut self.video,
            MediaKind::Youtube => &mut self.youtube,
        }
    }

    /// Files a playlist from the cooler (from an older version, which mixed
    /// kinds, or from the CLI) into each kind's playlist, skipping items it
    /// already has. Returns the kind of the first item, if any.
    pub fn absorb(&mut self, items: &[MediaItem], item_secs: f64) -> Option<MediaKind> {
        for item in items {
            let list = self.get_mut(MediaKind::of(item));
            if !list.items.iter().any(|i| same_source(i, item)) {
                if list.items.is_empty() {
                    list.item_secs = item_secs;
                }
                list.items.push(item.clone());
            }
        }
        items.first().map(MediaKind::of)
    }
}

fn same_source(a: &MediaItem, b: &MediaItem) -> bool {
    a.path == b.path && a.url == b.url
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn file(name: &str) -> MediaItem {
        MediaItem::file(PathBuf::from(name))
    }

    #[test]
    fn kinds_follow_the_extension() {
        assert_eq!(MediaKind::of(&file("a.PNG")), MediaKind::Image);
        assert_eq!(MediaKind::of(&file("a.Gif")), MediaKind::Gif);
        assert_eq!(MediaKind::of(&file("a.mp4")), MediaKind::Video);
        assert_eq!(
            MediaKind::of(&MediaItem::web("https://youtu.be/x".into())),
            MediaKind::Youtube
        );
    }

    #[test]
    fn absorbing_a_mixed_playlist_sorts_it_by_kind_once() {
        let mixed = [
            file("b.gif"),
            file("a.png"),
            file("c.mkv"),
            MediaItem::web("https://youtu.be/x".into()),
        ];
        let mut lists = Playlists::default();
        assert_eq!(lists.absorb(&mixed, 5.0), Some(MediaKind::Gif));
        assert_eq!(lists.absorb(&mixed, 5.0), Some(MediaKind::Gif));
        for kind in MediaKind::ALL {
            assert_eq!(lists.get(kind).items.len(), 1, "{kind:?}");
            assert_eq!(lists.get(kind).item_secs, 5.0);
        }
    }
}
