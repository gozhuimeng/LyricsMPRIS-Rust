pub mod lrclib;
pub mod lrcx;
pub mod musixmatch;

pub use lrclib::fetch_lyrics_from_lrclib;
pub use lrcx::fetch_lyrics_from_lrcx;
pub use musixmatch::fetch_lyrics_from_musixmatch_usertoken;
