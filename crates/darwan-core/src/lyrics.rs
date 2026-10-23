use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// One lyric line; `ms` is when it starts, None for lyrics without timing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Line {
    #[serde(rename = "t")]
    pub ms: Option<u64>,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Lyrics {
    pub synced: bool,
    pub lines: Vec<Line>,
}

// `[mm:ss]`, `[mm:ss.xx]` or `[mm:ss:xx]` at the start of a line, several for a line sung more than once. Tags such as
// `[ar:…]` and `[offset:…]` aren't times and are skipped; `[offset:+250]` shifts every line earlier by 250 ms, as
// the LRC format has it.
pub fn parse_lrc(text: &str) -> Vec<Line> {
    let mut offset: i64 = 0;
    let mut out = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some((tag, after)) = inner.split_once(']') else {
                break;
            };
            if let Some(v) = tag.strip_prefix("offset:") {
                offset = v.trim().parse().unwrap_or(0);
            }
            if let Some(ms) = timestamp(tag) {
                times.push(ms);
            }
            rest = after;
        }
        let text = rest.trim();
        for ms in times {
            out.push(Line {
                ms: Some(ms),
                text: text.to_string(),
            });
        }
    }
    for l in &mut out {
        l.ms = l.ms.map(|ms| (ms as i64 - offset).max(0) as u64);
    }
    out.sort_by_key(|l| l.ms);
    out
}

fn timestamp(tag: &str) -> Option<u64> {
    let (min, sec) = tag.split_once(':')?;
    let min: u64 = min.trim().parse().ok()?;
    let (whole, frac) = match sec.split_once(['.', ':']) {
        Some((w, f)) => (w, f),
        None => (sec, ""),
    };
    let whole: u64 = whole.parse().ok()?;
    if whole >= 60 || !frac.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    // ".5" is half a second, ".50" and ".500" too.
    let frac_ms = format!("{frac:0<3}")[..3].parse::<u64>().unwrap_or(0);
    Some((min * 60 + whole) * 1000 + frac_ms)
}

pub fn plain(text: &str) -> Vec<Line> {
    text.lines()
        .map(|l| Line {
            ms: None,
            text: l.trim().to_string(),
        })
        .collect()
}

// Timed lines win over plain text, whichever kind of tag holds them.
fn from_text(text: &str) -> Option<Lyrics> {
    let synced = parse_lrc(text);
    if !synced.is_empty() {
        return Some(Lyrics {
            synced: true,
            lines: synced,
        });
    }
    let lines = plain(text);
    lines.iter().any(|l| !l.text.is_empty()).then_some(Lyrics {
        synced: false,
        lines,
    })
}

// A `file://` URL as MPRIS players send it, or a plain path.
pub fn path_of(url: &str) -> PathBuf {
    let Some(rest) = url.strip_prefix("file://") else {
        return PathBuf::from(url);
    };
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match (bytes[i], rest.get(i + 1..i + 3)) {
            (b'%', Some(hex)) if u8::from_str_radix(hex, 16).is_ok() => {
                out.push(u8::from_str_radix(hex, 16).unwrap_or(0));
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

// ffprobe's tags for the file and its streams (Ogg and Opus keep theirs on the stream), lyric ones first by kind:
// synced, then anything named lyrics (LYRICS, UNSYNCEDLYRICS, lyrics-eng…).
fn tag_lyrics(file: &Path) -> Vec<String> {
    let Ok(out) = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-show_entries",
            "format_tags:stream_tags",
            "-of",
            "json",
        ])
        .arg(file)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return Vec::new();
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return Vec::new();
    };
    lyric_tags(&v)
}

fn lyric_tags(v: &serde_json::Value) -> Vec<String> {
    let mut tags: Vec<(&String, &serde_json::Value)> = Vec::new();
    if let Some(t) = v["format"]["tags"].as_object() {
        tags.extend(t.iter());
    }
    for s in v["streams"].as_array().into_iter().flatten() {
        if let Some(t) = s["tags"].as_object() {
            tags.extend(t.iter());
        }
    }
    let rank = |k: &str| {
        let k = k.to_ascii_lowercase();
        if k.contains("synced") && !k.contains("unsynced") {
            Some(0)
        } else if k.contains("lyric") {
            Some(1)
        } else {
            None
        }
    };
    let mut found: Vec<(u8, String)> = tags
        .into_iter()
        .filter_map(|(k, v)| Some((rank(k)?, v.as_str()?.to_string())))
        .collect();
    found.sort_by_key(|(r, _)| *r);
    found.into_iter().map(|(_, t)| t).collect()
}

// A `.lrc` beside the track wins, then the file's own tags.
pub fn for_file(file: &Path) -> Option<Lyrics> {
    let sidecar = file.with_extension("lrc");
    if let Some(l) = std::fs::read_to_string(&sidecar)
        .ok()
        .and_then(|t| from_text(&t))
    {
        return Some(l);
    }
    tag_lyrics(file).iter().find_map(|t| from_text(t))
}

// What a track is, as the player reports it, for looking its lyrics up online.
#[derive(Debug, Clone, Default)]
pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    // Seconds; 0 when the player doesn't say.
    pub length: f64,
}

// The first of several credited artists: "Alan Walker, Au/Ra & Tomine Harket" is filed under Alan Walker.
fn lead_artist(artist: &str) -> &str {
    let mut end = artist.len();
    for sep in [", ", " & ", " feat", " ft.", " x ", "; "] {
        if let Some(i) = artist.to_lowercase().find(sep) {
            end = end.min(i);
        }
    }
    artist[..end].trim()
}

fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}

// LRCLIB's answer for one track: timed lyrics when it has them, plain otherwise.
fn from_lrclib(v: &serde_json::Value) -> Option<Lyrics> {
    if v["instrumental"] == true {
        return None;
    }
    ["syncedLyrics", "plainLyrics"]
        .iter()
        .find_map(|k| v[k].as_str().and_then(from_text))
}

// Of LRCLIB's search results, the one closest in length (within 10 s when the length is known), timed first.
fn best_match(results: &[serde_json::Value], length: f64) -> Option<Lyrics> {
    let mut fit: Vec<&serde_json::Value> = results
        .iter()
        .filter(|r| {
            length <= 0.0
                || r["duration"]
                    .as_f64()
                    .is_none_or(|d| (d - length).abs() <= 10.0)
        })
        .collect();
    fit.sort_by(|a, b| {
        let gap = |r: &serde_json::Value| (r["duration"].as_f64().unwrap_or(length) - length).abs();
        let timed = |r: &serde_json::Value| r["syncedLyrics"].as_str().is_none();
        (timed(a), gap(a))
            .partial_cmp(&(timed(b), gap(b)))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    fit.into_iter().find_map(from_lrclib)
}

const ONLINE_TTL: std::time::Duration = std::time::Duration::from_secs(86400);

// lrclib.net, free and keyless: the exact track first, then a search on the lead artist. Answers, a miss included,
// are kept a day under `cache`, so a song is asked about at most once a day.
pub fn online(track: &Track, cache: &Path) -> Option<Lyrics> {
    // Without an artist ("Firefox is playing media") a search is a guess, and the title is no song.
    if track.title.trim().is_empty() || track.artist.trim().is_empty() {
        return None;
    }
    let key = format!(
        "{}\0{}\0{}\0{}",
        track.title,
        track.artist,
        track.album,
        track.length.round()
    );
    let file = cache.join(format!("{:016x}.json", crate::media::fnv1a(key.as_bytes())));
    let fresh = std::fs::metadata(&file)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.elapsed().ok())
        .is_some_and(|age| age < ONLINE_TTL);
    if fresh && let Ok(text) = std::fs::read_to_string(&file) {
        return serde_json::from_str::<Option<Lyrics>>(&text).ok().flatten();
    }
    let found = ask_lrclib(track);
    // A failed request (offline) isn't a miss: only an answer is kept.
    if let Ok(found) = &found {
        let _ = std::fs::create_dir_all(cache);
        if let Ok(text) = serde_json::to_string(found) {
            let _ = std::fs::write(&file, text);
        }
    }
    found.ok().flatten()
}

fn ask_lrclib(track: &Track) -> Result<Option<Lyrics>, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .user_agent(crate::wallpaper::online::USER_AGENT)
        .timeout_global(Some(std::time::Duration::from_secs(8)))
        .http_status_as_error(false)
        .build()
        .into();
    let get = |url: &str| -> Result<Option<serde_json::Value>, String> {
        let mut resp = agent.get(url).call().map_err(|e| e.to_string())?;
        match resp.status().as_u16() {
            404 => Ok(None),
            200 => {
                let text = resp
                    .body_mut()
                    .read_to_string()
                    .map_err(|e| e.to_string())?;
                serde_json::from_str(&text)
                    .map(Some)
                    .map_err(|e| e.to_string())
            }
            code => Err(format!("lrclib.net answered {code}")),
        }
    };
    // The exact lookup needs the album and the length; without them only the search is asked.
    if !track.album.is_empty() && track.length > 0.0 {
        let exact = format!(
            "https://lrclib.net/api/get?track_name={}&artist_name={}&album_name={}&duration={}",
            encode(&track.title),
            encode(&track.artist),
            encode(&track.album),
            track.length.round()
        );
        if let Some(l) = get(&exact)?.as_ref().and_then(from_lrclib) {
            return Ok(Some(l));
        }
    }
    let search = format!(
        "https://lrclib.net/api/search?track_name={}&artist_name={}",
        encode(&track.title),
        encode(lead_artist(&track.artist))
    );
    let results = get(&search)?.unwrap_or_default();
    Ok(best_match(
        results.as_array().map_or(&[][..], Vec::as_slice),
        track.length,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc_lines_are_timed_sorted_and_tags_are_not_lyrics() {
        let lines = parse_lrc(
            "[ar:Alan Walker]\n[00:16.44] We share no stories\n[00:13.75] We're not in love\n[01:02][02:10.5]Chorus\nplain line\n",
        );
        let got: Vec<(Option<u64>, &str)> = lines.iter().map(|l| (l.ms, l.text.as_str())).collect();
        assert_eq!(
            got,
            [
                (Some(13_750), "We're not in love"),
                (Some(16_440), "We share no stories"),
                (Some(62_000), "Chorus"),
                (Some(130_500), "Chorus"),
            ]
        );
    }

    #[test]
    fn an_offset_moves_every_line_earlier() {
        let lines = parse_lrc("[offset:+500]\n[00:01.00]a\n[00:00.20]b\n");
        assert_eq!(lines[0].ms, Some(0), "never before the start");
        assert_eq!(lines[1].ms, Some(500));
    }

    #[test]
    fn synced_tags_are_preferred_and_plain_lyrics_are_kept_as_a_fallback() {
        let v = serde_json::json!({
            "format": { "tags": { "LYRICS": "one\ntwo", "SYNCEDLYRICS": "[00:01.00] one", "title": "x" } },
            "streams": [{ "tags": { "lyrics-eng": "three" } }]
        });
        let tags = lyric_tags(&v);
        assert_eq!(tags[0], "[00:01.00] one");
        assert_eq!(tags.len(), 3);
        assert!(from_text(&tags[0]).unwrap().synced);
        let plain = from_text("one\n\ntwo").unwrap();
        assert!(!plain.synced);
        assert_eq!(plain.lines.len(), 3, "blank lines keep the verses apart");
        assert!(from_text("\n  \n").is_none());
    }

    #[test]
    fn a_sidecar_lrc_wins_over_tags() {
        let d = tempfile::tempdir().unwrap();
        let track = d.path().join("song.flac");
        std::fs::write(&track, b"not audio").unwrap();
        std::fs::write(d.path().join("song.lrc"), "[00:02.00] from the lrc").unwrap();
        let l = for_file(&track).unwrap();
        assert_eq!(l.lines[0].text, "from the lrc");
        assert!(for_file(&d.path().join("missing.flac")).is_none());
    }

    #[test]
    fn the_lead_artist_is_the_first_credited() {
        assert_eq!(
            lead_artist("Alan Walker, Au/Ra & Tomine Harket"),
            "Alan Walker"
        );
        assert_eq!(lead_artist("Daft Punk feat. Pharrell"), "Daft Punk");
        assert_eq!(lead_artist("Muse"), "Muse");
    }

    #[test]
    fn lrclib_results_pick_the_closest_length_and_prefer_timed_lyrics() {
        let r = serde_json::json!([
            { "duration": 239.0, "syncedLyrics": "[00:01.00] single edit", "plainLyrics": "single edit" },
            { "duration": 213.0, "syncedLyrics": null, "plainLyrics": "album plain" },
            { "duration": 211.0, "syncedLyrics": "[00:01.00] album timed", "plainLyrics": "x" },
            { "duration": 212.0, "instrumental": true }
        ]);
        let best = best_match(r.as_array().unwrap(), 212.0).unwrap();
        assert!(best.synced);
        assert_eq!(
            best.lines[0].text, "album timed",
            "the 239 s edit is too far off"
        );
        let plain = best_match(&r.as_array().unwrap()[1..2], 212.0).unwrap();
        assert!(!plain.synced, "plain lyrics when nothing timed fits");
        assert!(
            best_match(&r.as_array().unwrap()[3..], 212.0).is_none(),
            "instrumental"
        );
    }

    #[test]
    fn a_cached_online_answer_is_reused_misses_included() {
        let d = tempfile::tempdir().unwrap();
        let t = Track {
            title: "Song".into(),
            artist: "Band".into(),
            album: String::new(),
            length: 200.0,
        };
        let key = format!(
            "{}\0{}\0{}\0{}",
            t.title,
            t.artist,
            t.album,
            t.length.round()
        );
        let file = d
            .path()
            .join(format!("{:016x}.json", crate::media::fnv1a(key.as_bytes())));
        std::fs::write(&file, "null").unwrap();
        assert_eq!(online(&t, d.path()), None, "a cached miss asks nobody");
        let hit = Lyrics {
            synced: true,
            lines: parse_lrc("[00:01.00] cached"),
        };
        std::fs::write(&file, serde_json::to_string(&Some(hit.clone())).unwrap()).unwrap();
        assert_eq!(online(&t, d.path()), Some(hit));
        assert_eq!(
            online(&Track::default(), d.path()),
            None,
            "no title, no request"
        );
        let browser = Track {
            title: "Zen is playing media".into(),
            ..Track::default()
        };
        assert_eq!(online(&browser, d.path()), None, "no artist, no request");
    }

    #[test]
    fn mpris_urls_are_decoded_to_paths() {
        assert_eq!(
            path_of("file:///home/u/Music/Alan%20Walker/Au_Ra%20%26%20T.flac"),
            PathBuf::from("/home/u/Music/Alan Walker/Au_Ra & T.flac")
        );
        assert_eq!(path_of("/a/b.mp3"), PathBuf::from("/a/b.mp3"));
    }
}
