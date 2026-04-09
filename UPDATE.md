# 更新记录

## 2026-04-09

### 问题 1: 多艺术家处理

**现象**: 当艺术家字段包含多个艺术家时（如 `Sihan / 三Z-STUDIO / HOYO-MiX`），API 搜索失败。

**原因**: 多个艺术家被合并为一个字符串传入 API，而不是只使用第一个艺术家。

**修复**: 在 `src/mpris/metadata.rs` 中添加 `normalize_artist()` 函数，按 `/` 分割并只取第一个艺术家。

```rust
impl TrackMetadata {
    fn normalize_artist(artist: &str) -> String {
        artist
            .split('/')
            .next()
            .map(|s| s.trim())
            .unwrap_or_default()
            .to_string()
    }
}
```

### 问题 2: Album 包含 Track Name 导致 API 404

**现象**: 使用 lrclib API 搜索歌词时返回 404，但直接用 curl 测试 API 正常。

**原因**: 播放器报告的 Album 字段为 `绝区零-DAMIDAMI`，其中包含了 track name `DAMIDAMI`。lrclib API 对 album 参数要求严格，包含 track name 会导致 404。

**修复**: 在 `src/lyrics/providers/lrclib.rs` 的 `build_lrclib_url()` 中，当 album 包含 track name 时跳过 album 参数。

```rust
// Only include album if it doesn't contain the track name (avoids 404s from bad metadata)
if !album.is_empty() && !album.to_lowercase().contains(&title.to_lowercase()) {
    params.push(format!("album_name={}", urlencoding::encode(album)));
}
```

### 问题 3: 多艺术家依次尝试

**现象**: 当第一个艺术家搜索歌词失败时，不会尝试其他艺术家。

**原因**: 之前的修复只使用了第一个艺术家，忽略了其他艺术家。

**修复**: 在 `src/event.rs` 的 `fetch_api_lyrics()` 中，依次尝试所有艺术家，直到找到歌词。

```rust
// Get all artists to try: first artist, second artist, ..., empty string
let mut artists = meta.all_artists();
if artists.is_empty() || artists[0].is_empty() {
    artists.clear();
} else {
    artists.push(String::new()); // Try without artist as last resort
}

// Try each provider with each artist
for provider in providers {
    for artist in &artists {
        match try_provider_for_artist(provider, meta, artist, state).await {
            FetchResult::Success => return,
            // ...
        }
    }
}
```

### 问题 4: D-Bus 返回的多艺术家格式

**现象**: D-Bus 返回的艺术家数组只有一个元素，内容是 `"张杰 / HOYO-MiX"`（多个艺术家用 slash 连接在同一字符串中），而不是 `["张杰", "HOYO-MiX"]`（多个独立元素）。

**原因**: 不同播放器实现方式不同。有的播放器把多艺术家放在一个字符串里，有的分成多个数组元素。

**修复**: 在 `src/mpris/metadata.rs` 中添加 `extract_artists_from_string()` 函数，对每个数组元素都尝试按 `/` 分割提取所有艺术家。

```rust
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
```

### URL 参数说明

- `artist_name`: 艺术家名称
- `track_name`: 歌曲名称
- `album_name`: 专辑名称（可选）
- `duration`: 歌曲时长（秒），用于精确匹配

### 保留的二进制文件

- `release/lyricsmpris` - 最新版本（含问题1-4修复）
- `release/lyricsmpris-20260409-194916` - 旧版本（修复前）
- `release/lyricsmpris-20260409-210507-fix` - 调试版本（带详细日志）
- `release/lyricsmpris-20260409-214000-multiartist` - 多艺术家依次尝试修复前的版本
- `release/lyricsmpris-20260409-222300-fix` - 问题4修复后（含调试日志注释版）

### 待提交的文件

```
M UPDATE.md                        # 更新记录
M src/lyrics/providers/lrclib.rs  # Album 过滤修复
M src/mpris/metadata.rs            # 艺术家列表支持（含问题4修复）
M src/event.rs                    # 多艺术家尝试逻辑
M release/lyricsmpris            # 编译后的二进制
```
