//! Track metadata parsing and querying for MPRIS.

use crate::mpris::connection::{get_dbus_conn, MprisError};
use std::collections::HashMap;
use zbus::{proxy, zvariant};
use zvariant::{OwnedValue, Type};

/// Track metadata from MPRIS player
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub length: Option<f64>,
    pub spotify_id: Option<String>,
    #[doc(hidden)]
    pub all_artists: Vec<String>,
}

/// Extracts all artists from a string that may contain multiple artists separated by `/`.
///
/// Handles formats like "Artist1 / Artist2" or "Artist1/Artist2".
fn extract_artists_from_string(s: &str) -> Vec<String> {
    if s.contains('/') {
        s.split('/')
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect()
    } else if !s.is_empty() {
        vec![s.to_string()]
    } else {
        Vec::new()
    }
}

impl TrackMetadata {
    /// Normalizes artist string by taking the first artist when multiple are combined with `/`.
    ///
    /// Handles formats like "Artist1 / Artist2" or "Artist1/Artist2" by splitting
    /// on `/` and returning only the first part, trimmed of whitespace.
    fn normalize_artist(artist: &str) -> String {
        artist
            .split('/')
            .next()
            .map(|s| s.trim())
            .unwrap_or_default()
            .to_string()
    }

    /// Parses Kugou player title format.
    ///
    /// Format: `title_artists_uselessInfo_lyrics`
    /// Example: `Where Do We Go_ZHANGYE、Steve Aoki、Rosie Darling_...歌词...`
    ///
    /// Returns (title, artists) or None if not Kugou format.
    fn parse_kugou_title(title: &str) -> Option<(String, Vec<String>)> {
        if !title.contains("歌曲下载_酷狗音乐") {
            return None;
        }

        let parts: Vec<&str> = title.split('_').collect();
        if parts.len() < 2 {
            return None;
        }

        let song_title = parts[0].trim().to_string();
        let artists_str = parts[1];

        // Artists are separated by `、` in Kugou format
        let artists: Vec<String> = artists_str
            .split('、')
            .map(|a| a.trim().to_string())
            .filter(|a| !a.is_empty())
            .collect();

        if artists.is_empty() {
            return None;
        }

        Some((song_title, artists))
    }

    /// Extracts all artists from a value that may be:
    /// 1. A single artist string like "Artist1"
    /// 2. Multiple artists in one string like "Artist1 / Artist2"
    /// 3. Multiple artists in an array
    fn extract_artists(value: Option<String>) -> Vec<String> {
        match value {
            Some(s) if s.contains('/') => {
                // String contains multiple artists separated by /
                s.split('/')
                    .map(|a| a.trim().to_string())
                    .filter(|a| !a.is_empty())
                    .collect()
            }
            Some(s) if !s.is_empty() => vec![s],
            _ => Vec::new(),
        }
    }

    /// Returns all artists extracted from the original metadata.
    ///
    /// Used when the first artist fails to find lyrics - tries subsequent artists.
    pub fn all_artists(&self) -> Vec<String> {
        self.all_artists.clone()
    }
}

/// Internal metadata structure matching MPRIS specification
/// 
/// Uses zvariant's DeserializeDict to properly handle D-Bus dictionary types.
#[derive(Debug, Type)]
#[zvariant(signature = "a{sv}")]
struct MprisMetadata {
    #[zvariant(rename = "xesam:title")]
    title: Option<String>,
    #[zvariant(rename = "xesam:artist")]
    artist: Option<Vec<String>>,
    #[zvariant(rename = "xesam:album")]
    album: Option<Vec<String>>,
    #[zvariant(rename = "mpris:length")]
    length: Option<i64>,
    #[zvariant(rename = "mpris:trackid")]
    trackid: Option<String>,
}

impl From<MprisMetadata> for TrackMetadata {
    fn from(md: MprisMetadata) -> Self {
        let title_raw = md.title.unwrap_or_default();
        
        // Check if this is Kugou player title format
        let (title, all_artists) = match TrackMetadata::parse_kugou_title(&title_raw) {
            Some((parsed_title, parsed_artists)) => (parsed_title, parsed_artists),
            None => {
                // Extract all artists from array, handling both ["A1", "A2"] and ["A1 / A2"] formats
                let artists = md.artist
                    .map(|arr| {
                        arr.into_iter()
                            .map(|s| extract_artists_from_string(&s))
                            .flatten()
                            .collect::<Vec<String>>()
                    })
                    .unwrap_or_default();
                (title_raw, artists)
            }
        };
        
        let artist = all_artists.first()
            .cloned()
            .unwrap_or_default();
        
        let album = md
            .album
            .and_then(|albums| albums.into_iter().next())
            .unwrap_or_default();
        
        // Convert microseconds to seconds
        let length = md.length.map(|microsecs| microsecs as f64 / 1_000_000.0);
        
        // Extract Spotify ID from track ID
        let spotify_id = md.trackid.and_then(|trackid| {
            // Try extracting from path like "/org/mpris/MediaPlayer2/Track/spotify/track/ID"
            if let Some(id) = trackid.rsplit('/').next()
                && !id.is_empty() && id.len() == 22 {
                    return Some(id.to_string());
                }
            
            // Try extracting from spotify:track:ID format
            if let Some(idx) = trackid.find("spotify:track:") {
                let id = &trackid[idx + "spotify:track:".len()..];
                if !id.is_empty() {
                    return Some(id.to_string());
                }
            }
            
            None
        });

        TrackMetadata {
            title,
            artist,
            album,
            length,
            spotify_id,
            all_artists,
        }
    }
}

/// Extract metadata from a raw D-Bus property map
/// 
/// This is used for signal handlers where we receive raw variant maps.
pub fn extract_metadata(map: &HashMap<String, OwnedValue>) -> TrackMetadata {
    // Helper to extract string from variant
    let get_string = |key: &str| -> Option<String> {
        map.get(key).and_then(|v| {
            <&str>::try_from(v).ok().map(String::from)
        })
    };

    // Helper to extract string array from variant
    let get_string_array = |key: &str| -> Option<Vec<String>> {
        map.get(key).and_then(|v| {
            // Try to deserialize directly from OwnedValue as array
            zvariant::Array::try_from(v.clone())
                .ok()
                .and_then(|arr| {
                    arr.iter()
                        .map(|elem| <&str>::try_from(elem).ok().map(String::from))
                        .collect::<Option<Vec<String>>>()
                })
        })
    };

    // Helper to extract integer from variant
    let get_i64 = |key: &str| -> Option<i64> {
        map.get(key).and_then(|v| {
            // Try both i64 and u64
            i64::try_from(v).ok().or_else(|| {
                u64::try_from(v).ok().map(|u| u as i64)
            })
        })
    };

    let title_raw = get_string("xesam:title").unwrap_or_default();
    
    // Check if this is Kugou player title format
    // Format: title_artists_uselessInfo_lyrics
    // Example: "Where Do We Go_ZHANGYE、Steve Aoki、Rosie Darling_...歌词..."
    let (title, all_artists_extracted) = match TrackMetadata::parse_kugou_title(&title_raw) {
        Some((parsed_title, parsed_artists)) => {
            // tracing::debug!(title = %parsed_title, artists = ?parsed_artists, "Kugou title format detected");
            (parsed_title, parsed_artists)
        }
        None => {
            // Artist: try array first, fallback to string
            // Handle both formats: ["Artist1", "Artist2"] and ["Artist1 / Artist2"]
            let artists_from_array = get_string_array("xesam:artist").map(|arr| {
                arr.into_iter()
                    .map(|s| extract_artists_from_string(&s))
                    .flatten()
                    .collect::<Vec<String>>()
            });
            
            let artist_raw = artists_from_array
                .as_ref()
                .map(|v| v.first().cloned())
                .flatten()
                .or_else(|| get_string("xesam:artist"))
                .unwrap_or_default();
            
            let all_artists = artists_from_array
                .or_else(|| get_string("xesam:artist").map(|s| extract_artists_from_string(&s)))
                .unwrap_or_default();
            
            (title_raw, all_artists)
        }
    };
    
    let artist = all_artists_extracted.first().cloned().unwrap_or_default();
    
    // Album: try array first, fallback to string
    let album = get_string_array("xesam:album")
        .and_then(|arr| arr.into_iter().next())
        .or_else(|| get_string("xesam:album"))
        .unwrap_or_default();
    
    let length = get_i64("mpris:length").map(|microsecs| microsecs as f64 / 1_000_000.0);

    let spotify_id = get_string("mpris:trackid").and_then(|trackid| {
        // Try extracting from path
        if let Some(id) = trackid.rsplit('/').next()
            && !id.is_empty() && id.len() == 22 {
                return Some(id.to_string());
            }
        
        // Try spotify:track: format
        if let Some(idx) = trackid.find("spotify:track:") {
            let id = &trackid[idx + "spotify:track:".len()..];
            if !id.is_empty() {
                return Some(id.to_string());
            }
        }
        
        None
    });

    TrackMetadata {
        title,
        artist,
        album,
        length,
        spotify_id,
        all_artists: all_artists_extracted,
    }
}

/// MPRIS MediaPlayer2.Player interface proxy
#[proxy(
    interface = "org.mpris.MediaPlayer2.Player",
    default_path = "/org/mpris/MediaPlayer2"
)]
trait MediaPlayer2Player {
    #[zbus(property)]
    fn metadata(&self) -> zbus::Result<HashMap<String, OwnedValue>>;
}

/// Query metadata for a specific MPRIS player service
pub async fn get_metadata(service: &str) -> Result<TrackMetadata, MprisError> {
    if service.is_empty() {
        return Ok(TrackMetadata::default());
    }

    let conn = get_dbus_conn().await?;
    
    let proxy = MediaPlayer2PlayerProxy::builder(&conn)
        .destination(service)?
        .build()
        .await?;

    match proxy.metadata().await {
        Ok(metadata_map) => Ok(extract_metadata(&metadata_map)),
        Err(_) => Ok(TrackMetadata::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata_conversion() {
        let md = MprisMetadata {
            title: Some("Test Song".to_string()),
            artist: Some(vec!["Artist 1".to_string(), "Artist 2".to_string()]),
            album: Some(vec!["Test Album".to_string()]),
            length: Some(180_000_000), // 180 seconds in microseconds
            trackid: None,
        };

        let track: TrackMetadata = md.into();
        assert_eq!(track.title, "Test Song");
        assert_eq!(track.artist, "Artist 1");
        assert_eq!(track.album, "Test Album");
        assert_eq!(track.length, Some(180.0));
    }

    #[test]
    fn test_normalize_artist_single() {
        assert_eq!(TrackMetadata::normalize_artist("Artist One"), "Artist One");
    }

    #[test]
    fn test_normalize_artist_with_slash_space() {
        assert_eq!(TrackMetadata::normalize_artist("Artist One / Artist Two"), "Artist One");
    }

    #[test]
    fn test_normalize_artist_with_slash_no_space() {
        assert_eq!(TrackMetadata::normalize_artist("Artist One/Artist Two"), "Artist One");
    }

    #[test]
    fn test_normalize_artist_trim_whitespace() {
        assert_eq!(TrackMetadata::normalize_artist("  Artist One  "), "Artist One");
        assert_eq!(TrackMetadata::normalize_artist("  Artist One  / Artist Two  "), "Artist One");
    }

    #[test]
    fn test_all_artists() {
        let meta = TrackMetadata {
            title: "Test".to_string(),
            artist: "Artist One / Artist Two".to_string(),
            album: "".to_string(),
            length: None,
            spotify_id: None,
            all_artists: vec!["Artist One".to_string(), "Artist Two".to_string()],
        };

        assert_eq!(meta.all_artists(), vec!["Artist One", "Artist Two"]);
    }

    #[test]
    fn test_all_artists_single() {
        let meta = TrackMetadata {
            title: "Test".to_string(),
            artist: "Artist One".to_string(),
            album: "".to_string(),
            length: None,
            spotify_id: None,
            all_artists: vec!["Artist One".to_string()],
        };

        assert_eq!(meta.all_artists(), vec!["Artist One"]);
    }

    #[test]
    fn test_all_artists_empty() {
        let meta = TrackMetadata {
            title: "Test".to_string(),
            artist: "".to_string(),
            album: "".to_string(),
            length: None,
            spotify_id: None,
            all_artists: Vec::new(),
        };

        assert_eq!(meta.all_artists(), Vec::<String>::new());
    }

    #[test]
    fn test_parse_kugou_title() {
        let title = "Where Do We Go_ZHANGYE、Steve Aoki、Rosie Darling_高音质在线试听_Where Do We Go歌词|歌曲下载_酷狗音乐";
        let result = TrackMetadata::parse_kugou_title(title);
        assert!(result.is_some());
        let (parsed_title, artists) = result.unwrap();
        assert_eq!(parsed_title, "Where Do We Go");
        assert_eq!(artists, vec!["ZHANGYE", "Steve Aoki", "Rosie Darling"]);
    }

    #[test]
    fn test_parse_kugou_title_non_kugou() {
        let title = "Normal Title_Artist";
        let result = TrackMetadata::parse_kugou_title(title);
        assert!(result.is_none());
    }
}
