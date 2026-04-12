use serde_json::Value;

use crate::lyrics::parse::parse_synced_lyrics;
use crate::lyrics::types::{http_client, LyricsError, ProviderResult};

/// Fetch synced lyrics from lrc.cx public API.
///
/// API: https://api.lrc.cx/lyrics?title=<title>&artist=<artist>
pub async fn fetch_lyrics_from_lrcx(
    artist: &str,
    title: &str,
    _album: &str,
    _duration: Option<f64>,
) -> ProviderResult {
    let url = format!(
        "https://api.lrc.cx/lyrics?title={}&artist={}",
        urlencoding::encode(title),
        urlencoding::encode(artist)
    );

    let resp = http_client()
        .get(&url)
        .header("User-Agent", "LyricsMPRIS/1.0")
        .send()
        .await?;

    if resp.status().as_u16() == 404 {
        // [DEBUG-LOG]
        println!("查询失败：{}", url);
        // [/DEBUG-LOG]
        return Ok((Vec::new(), None));
    }

    if !resp.status().is_success() {
        let err = format!("lrcx: HTTP {}", resp.status());
        // [DEBUG-LOG]
        println!("查询失败：{} | {}", url, err);
        // [/DEBUG-LOG]
        return Err(LyricsError::Api(err));
    }

    let body = resp.text().await?;
    let trimmed = body.trim();
    if trimmed.is_empty() {
        // [DEBUG-LOG]
        println!("查询失败：{} | 空响应", url);
        // [/DEBUG-LOG]
        return Ok((Vec::new(), None));
    }

    let parsed_lrc = parse_synced_lyrics(trimmed);
    if !parsed_lrc.is_empty() {
        // [DEBUG-LOG]
        println!("查询成功：{}\n------------------", url);
        // [/DEBUG-LOG]
        return Ok((parsed_lrc, Some(trimmed.to_string())));
    }

    // Defensive fallback: some gateways may wrap payload in JSON.
    if let Ok(json) = serde_json::from_str::<Value>(trimmed)
        && let Some(raw_lrc) = extract_lrc_from_json(&json)
    {
        let parsed = parse_synced_lyrics(raw_lrc);
        if !parsed.is_empty() {
            // [DEBUG-LOG]
            println!("查询成功：{}\n------------------", url);
            // [/DEBUG-LOG]
            return Ok((parsed, Some(raw_lrc.to_string())));
        }
    }

    // [DEBUG-LOG]
    println!("查询失败：{} | 解析失败", url);
    // [/DEBUG-LOG]
    Ok((Vec::new(), None))
}

fn extract_lrc_from_json(json: &Value) -> Option<&str> {
    const KEYS: [&str; 5] = ["lyrics", "lyric", "lrc", "syncedLyrics", "synced_lyrics"];

    for key in KEYS {
        if let Some(text) = json.get(key).and_then(Value::as_str) {
            return Some(text);
        }
    }

    None
}
