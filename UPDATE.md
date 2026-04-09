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

### 保留的二进制文件

- `release/lyricsmpris` - 修复后的最新版本
- `release/lyricsmpris-20260409-194916` - 旧版本（修复前）
- `release/lyricsmpris-20260409-210507-fix` - 调试版本（带详细日志）

### 待提交的文件

```
M src/lyrics/providers/lrclib.rs  # Album 过滤修复
M src/mpris/metadata.rs          # 艺术家规范化
M release/lyricsmpris            # 编译后的二进制
```
