use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::filter::{self, Allowed};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Source {
    Wallhaven,
    Bing,
    Commons,
    Walls,
}

impl Source {
    pub const ALL: [Source; 4] = [
        Source::Wallhaven,
        Source::Bing,
        Source::Commons,
        Source::Walls,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Source::Wallhaven => "wallhaven",
            Source::Bing => "bing",
            Source::Commons => "commons",
            Source::Walls => "walls",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Source::Wallhaven => "Wallhaven",
            Source::Bing => "Bing",
            Source::Commons => "Wikimedia Commons",
            Source::Walls => "dharmx/walls",
        }
    }

    pub fn parse(s: &str) -> Option<Source> {
        Source::ALL.into_iter().find(|x| x.id() == s)
    }

    // Wallhaven is searched by its server, dharmx/walls by Darwan; the others are curated collections.
    pub fn searchable(self) -> bool {
        matches!(self, Source::Wallhaven | Source::Walls)
    }
}

// One wallpaper a source offers, with what the UI must show as credit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub source: Source,
    pub id: String,
    pub title: String,
    pub author: String,
    // The licence or terms line the source requires shown, e.g. "CC BY-SA 4.0" or "© Getty Images".
    pub licence: String,
    pub licence_url: String,
    pub page: String,
    pub full: String,
    pub thumb: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub colours: Vec<String>,
}

impl Found {
    fn extension(&self) -> &str {
        let path = self.full.split('?').next().unwrap_or(&self.full);
        match path
            .rsplit('.')
            .next()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("png") => "png",
            Some("webp") => "webp",
            _ => "jpg",
        }
    }

    // Unique across sources (Commons' ids are page numbers, dharmx/walls' paths).
    pub fn key(&self) -> String {
        format!("{}:{}", self.source.id(), self.id)
    }

    // The name in the wallpaper folder: the source and its id, so a second download is the same file.
    pub fn file_name(&self) -> String {
        let safe: String = self
            .id
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        format!("{}-{safe}.{}", self.source.id(), self.extension())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    Popular,
    Latest,
    Random,
}

#[derive(Debug, Clone, Default)]
pub struct Query {
    pub text: String,
    pub sort: Sort,
    // Wallhaven's ratios: "16x9", "21x9", "landscape".
    pub ratio: Option<String>,
    pub atleast: Option<(u32, u32)>,
    // A Commons subject chip; see COMMONS.
    pub topic: Option<String>,
    pub page: u32,
}

#[derive(Debug, Clone, Default)]
pub struct Page {
    pub items: Vec<Found>,
    // Why nothing was asked: the search text matched a refused term.
    pub refused: Option<String>,
    pub more: bool,
}

// Commons subject chips and their featured categories: subjects without people, war or predation.
pub const COMMONS: &[(&str, &[&str])] = &[
    (
        "nature",
        &[
            "Featured pictures of mountains",
            "Featured pictures of forests",
            "Featured pictures of coasts",
            "Featured pictures of bodies of water",
            "Featured pictures of islands",
            "Featured pictures of volcanoes",
            "Featured pictures of caves",
            "Featured pictures of plants",
        ],
    ),
    (
        "space",
        &[
            "Featured pictures of astronomy",
            "Featured pictures taken from satellites",
        ],
    ),
    (
        "city",
        &[
            "Featured pictures of cityscapes",
            "Featured pictures of architecture",
        ],
    ),
    ("night", &["Featured night photography"]),
];

// Explore's topic chips as Wallhaven searches; a topic in an optional group shows only once the user allows it.
pub const TOPICS: &[(&str, &str, Option<&str>)] = &[
    ("Nature", "nature", None),
    ("Space", "space", None),
    ("City", "city", None),
    ("Mountains", "mountains", None),
    ("Ocean", "sea", None),
    ("Forest", "forest", None),
    ("Minimal", "minimalism", None),
    ("Abstract", "abstract", None),
    ("Cars", "car", None),
    ("Sci-fi", "science fiction", None),
    ("Fantasy", "fantasy art", None),
    ("Cats", "cats", None),
    ("Monochrome", "monochrome", None),
    ("Anime", "anime", Some("anime")),
    ("Video games", "video games", Some("games")),
];

pub const USER_AGENT: &str = concat!(
    "Darwan/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/mah3uz/darwan; mah3uz@gmail.com)"
);

// Wallhaven allows 45 requests a minute; Darwan keeps to 40.
struct Limit {
    per_minute: usize,
    sent: Mutex<VecDeque<Instant>>,
}

impl Limit {
    fn wait(&self) {
        loop {
            let mut sent = self.sent.lock().unwrap();
            let now = Instant::now();
            while sent
                .front()
                .is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60))
            {
                sent.pop_front();
            }
            if sent.len() < self.per_minute {
                sent.push_back(now);
                return;
            }
            let wait = Duration::from_secs(60).saturating_sub(now.duration_since(sent[0]));
            drop(sent);
            std::thread::sleep(wait.max(Duration::from_millis(50)));
        }
    }
}

static WALLHAVEN: Limit = Limit {
    per_minute: 40,
    sent: Mutex::new(VecDeque::new()),
};

pub struct Client {
    agent: ureq::Agent,
    cache: PathBuf,
}

impl Client {
    // `cache`: ~/.cache/darwan/online, for responses and thumbnails.
    pub fn new(cache: PathBuf) -> Self {
        let config = ureq::Agent::config_builder()
            .user_agent(USER_AGENT)
            .timeout_global(Some(Duration::from_secs(30)))
            .build();
        Client {
            agent: config.into(),
            cache,
        }
    }

    fn get(&self, url: &str) -> Result<String, String> {
        if url.starts_with("https://wallhaven.cc/") {
            WALLHAVEN.wait();
        }
        // Wallhaven sits behind Cloudflare, whose 52x errors pass in a moment: one retry.
        let mut resp = match self.agent.get(url).call() {
            Err(ureq::Error::StatusCode(code)) if code >= 500 => {
                std::thread::sleep(Duration::from_secs(1));
                self.agent.get(url).call()
            }
            other => other,
        }
        .map_err(|e| format!("{}: {e}", host(url)))?;
        resp.body_mut()
            .with_config()
            .limit(20 * 1024 * 1024)
            .read_to_string()
            .map_err(|e| format!("{}: {e}", host(url)))
    }

    fn response_file(&self, key: &str) -> PathBuf {
        self.cache
            .join("responses")
            .join(format!("{:016x}.json", crate::media::fnv1a(key.as_bytes())))
    }

    // A response kept for `ttl`, so browsing doesn't ask the source again.
    fn cached(&self, key: &str, ttl: Duration, url: &str) -> Result<String, String> {
        let file = self.response_file(key);
        if let Ok(meta) = std::fs::metadata(&file)
            && meta
                .modified()
                .ok()
                .and_then(|m| m.elapsed().ok())
                .is_some_and(|age| age < ttl)
            && let Ok(text) = std::fs::read_to_string(&file)
        {
            return Ok(text);
        }
        let text = self.get(url)?;
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&file, &text);
        }
        Ok(text)
    }

    fn fetch_to(&self, url: &str, target: &Path) -> Result<(), String> {
        let resp = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("{}: {e}", host(url)))?;
        let dir = target.parent().ok_or("no folder")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let mut tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
        std::io::copy(&mut resp.into_body().into_reader(), &mut tmp)
            .map_err(|e| format!("{}: {e}", host(url)))?;
        tmp.persist(target).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn search(&self, source: Source, q: &Query, allowed: &Allowed) -> Result<Page, String> {
        self.search_each(source, q, allowed, &mut |_| {})
    }

    // As `search`, telling `each` about every result.
    pub fn search_each(
        &self,
        source: Source,
        q: &Query,
        allowed: &Allowed,
        each: &mut dyn FnMut(&Found),
    ) -> Result<Page, String> {
        let page = self.search_all(source, q, allowed)?;
        page.items.iter().for_each(&mut *each);
        Ok(page)
    }

    fn search_all(&self, source: Source, q: &Query, allowed: &Allowed) -> Result<Page, String> {
        if let Some(term) = filter::refusal(&q.text, allowed) {
            return Ok(Page {
                refused: Some(term.to_string()),
                ..Page::default()
            });
        }
        match source {
            Source::Wallhaven if wallhaven_skips(q) => Ok(Page::default()),
            Source::Wallhaven => self.wallhaven(q, allowed),
            // Bing keeps about two weeks, all on the first page.
            Source::Bing if q.page > 1 => Ok(Page::default()),
            Source::Bing => {
                let day = Duration::from_secs(6 * 3600);
                let mut items = Vec::new();
                for idx in [0, 7] {
                    let url = format!(
                        "https://www.bing.com/HPImageArchive.aspx?format=js&idx={idx}&n=8&mkt={}",
                        market()
                    );
                    items.extend(parse_bing(&self.cached(&url, day, &url)?));
                }
                items.dedup_by(|a, b| a.id == b.id);
                Ok(keep(items, allowed))
            }
            Source::Commons => self.commons(q, allowed),
            Source::Walls => self.walls(q, allowed),
        }
    }

    fn wallhaven(&self, q: &Query, allowed: &Allowed) -> Result<Page, String> {
        let text = filter::clean_query(&q.text);
        let query = format!("{text} {}", filter::wallhaven_exclusions(allowed))
            .trim()
            .to_string();
        let sorting = match q.sort {
            Sort::Popular => "toplist",
            Sort::Latest => "date_added",
            Sort::Random => "random",
        };
        let (w, h) = q.atleast.unwrap_or((1920, 1080));
        let mut url = format!(
            "https://wallhaven.cc/api/v1/search?categories={}&purity=100&sorting={sorting}&topRange=1y&atleast={}x{}&page={}&q={}",
            filter::wallhaven_categories(allowed),
            w.max(1920),
            h.max(1080),
            q.page.max(1),
            encode(&query)
        );
        if let Some(r) = &q.ratio {
            url.push_str(&format!("&ratios={}", encode(r)));
        }
        let body = if q.sort == Sort::Random {
            self.get(&url)?
        } else {
            self.cached(&url, Duration::from_secs(3600), &url)?
        };
        let (items, more) = parse_wallhaven_search(&body, allowed);
        Ok(Page {
            items,
            refused: None,
            more,
        })
    }

    // The whole repository listing, kept a day. GitHub allows 60 unauthenticated requests an hour per address and
    // that may already be spent, so an old copy is better than an error.
    fn walls(&self, q: &Query, allowed: &Allowed) -> Result<Page, String> {
        let key = "walls tree";
        let text = match self.cached(key, Duration::from_secs(86400), WALLS_TREE) {
            Ok(text) => text,
            Err(e) => std::fs::read_to_string(self.response_file(key)).map_err(|_| e)?,
        };
        let mut items = parse_walls(&text, allowed);
        let words = query_words(&format!("{} {}", q.text, q.topic.as_deref().unwrap_or("")));
        items.retain(|f| walls_match(&f.id, &words));
        let day = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() / 86400);
        let salt = if q.sort == Sort::Random {
            "random"
        } else {
            "day"
        };
        Ok(walls_page(items, day, salt, q.page))
    }

    fn commons(&self, q: &Query, allowed: &Allowed) -> Result<Page, String> {
        let topic = q.topic.as_deref().unwrap_or("nature");
        let cats = COMMONS
            .iter()
            .find(|(t, _)| *t == topic)
            .map(|(_, c)| *c)
            .ok_or_else(|| format!("no Commons subject {topic}"))?;
        let mut items = Vec::new();
        let mut more = false;
        for cat in cats {
            // Page N follows the API's continue token N-1 times; every step is cached, so paging on costs one request.
            let mut token: Option<String> = None;
            let mut text = String::new();
            for step in 0..q.page.max(1) {
                if step > 0 && token.is_none() {
                    text.clear();
                    break;
                }
                let mut url = format!(
                    "https://commons.wikimedia.org/w/api.php?action=query&format=json&formatversion=2&generator=categorymembers&gcmtitle={}&gcmtype=file&gcmlimit=50&prop=imageinfo|categories&cllimit=max&clshow=!hidden&iiprop=url|size|mime|extmetadata&iiurlwidth=500&iiextmetadatafilter=LicenseShortName|LicenseUrl|Artist|ObjectName",
                    encode(&format!("Category:{cat}"))
                );
                if let Some(t) = &token {
                    url.push_str(&format!("&gcmcontinue={}", encode(t)));
                }
                // Commons asks for one request at a time, which this loop is.
                text = self.cached(&url, Duration::from_secs(86400), &url)?;
                token = serde_json::from_str::<Value>(&text)
                    .ok()
                    .and_then(|v| v["continue"]["gcmcontinue"].as_str().map(str::to_string));
            }
            more |= token.is_some();
            items.extend(parse_commons(&text, allowed));
        }
        let mut page = keep(items, allowed);
        page.more = more;
        if q.sort == Sort::Random {
            let seed = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            page.items
                .sort_by_key(|f| crate::media::fnv1a(format!("{seed}{}", f.id).as_bytes()));
        }
        Ok(page)
    }

    // The grid's picture for `found`, from the cache when it's there.
    pub fn thumbnail(&self, found: &Found) -> Result<PathBuf, String> {
        let file = self.cache.join("thumbs").join(format!(
            "{}-{}.jpg",
            found.source.id(),
            crate::media::fnv1a(found.id.as_bytes())
        ));
        if file.is_file() {
            return Ok(file);
        }
        if found.source == Source::Walls {
            // dharmx/walls has no thumbnails, and its originals average 1 MB: the preview and the download reuse this one.
            let original = self.full(found)?;
            shrink(&original, &file)?;
        } else {
            self.fetch_to(&found.thumb, &file)?;
        }
        Ok(file)
    }

    fn full_cache(&self, found: &Found) -> PathBuf {
        self.cache.join("full").join(found.file_name())
    }

    // The full-size picture for a preview, kept until it's downloaded or the cache is trimmed.
    pub fn full(&self, found: &Found) -> Result<PathBuf, String> {
        let file = self.full_cache(found);
        if !file.is_file() {
            self.fetch_to(&found.full, &file)?;
            self.trim_full();
        }
        Ok(file)
    }

    // Previews not downloaded are kept for later, up to about 600 MB, the oldest going first.
    fn trim_full(&self) {
        let Ok(entries) = std::fs::read_dir(self.cache.join("full")) else {
            return;
        };
        let mut files: Vec<(SystemTime, u64, PathBuf)> = entries
            .flatten()
            .filter_map(|e| {
                let m = e.metadata().ok()?;
                Some((m.modified().ok()?, m.len(), e.path()))
            })
            .collect();
        files.sort();
        let mut total: u64 = files.iter().map(|f| f.1).sum();
        for (_, len, path) in files {
            if total <= 600 * 1024 * 1024 {
                break;
            }
            if std::fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }

    // Into the wallpaper folder under a stable name, with its credit kept beside Darwan's data.
    pub fn download(
        &self,
        found: &Found,
        folder: &Path,
        credits: &Path,
    ) -> Result<PathBuf, String> {
        let target = folder.join(found.file_name());
        if !target.is_file() {
            // A previewed picture moves in rather than being fetched again.
            let previewed = self.full_cache(found);
            let moved = previewed.is_file()
                && std::fs::create_dir_all(folder).is_ok()
                && (std::fs::rename(&previewed, &target).is_ok()
                    || std::fs::copy(&previewed, &target).is_ok());
            if !moved {
                self.fetch_to(&found.full, &target)?;
            }
        }
        save_credit(credits, &target, found);
        Ok(target)
    }
}

fn host(url: &str) -> &str {
    url.split('/').nth(2).unwrap_or(url)
}

fn keep(items: Vec<Found>, allowed: &Allowed) -> Page {
    Page {
        items: items
            .into_iter()
            .filter(|f| filter::refusal(&format!("{} {}", f.title, f.licence), allowed).is_none())
            .collect(),
        refused: None,
        more: false,
    }
}

fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn market() -> String {
    let lang = std::env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| std::env::var("LANG").ok())
        .unwrap_or_default();
    let tag = lang.split('.').next().unwrap_or("").replace('_', "-");
    if tag.len() == 5 { tag } else { "en-US".into() }
}

fn s(v: &Value, k: &str) -> String {
    v.get(k).and_then(Value::as_str).unwrap_or("").to_string()
}

fn n(v: &Value, k: &str) -> u64 {
    v.get(k).and_then(Value::as_u64).unwrap_or(0)
}

// Only SFW results, general or (when people are allowed) people; never anime, whatever the query was.
pub fn parse_wallhaven_search(text: &str, allowed: &Allowed) -> (Vec<Found>, bool) {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return (Vec::new(), false);
    };
    let more = n(&v["meta"], "current_page") < n(&v["meta"], "last_page");
    let items = v["data"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|d| {
            s(d, "purity") == "sfw"
                && match s(d, "category").as_str() {
                    "general" => true,
                    "people" => allowed.has("people"),
                    _ => false,
                }
        })
        .map(|d| Found {
            source: Source::Wallhaven,
            id: s(d, "id"),
            title: String::new(),
            author: String::new(),
            licence: "Images belong to their owners; for personal use".into(),
            licence_url: "https://wallhaven.cc/terms".into(),
            page: s(d, "url"),
            full: s(d, "path"),
            thumb: s(&d["thumbs"], "large"),
            width: n(d, "dimension_x") as u32,
            height: n(d, "dimension_y") as u32,
            bytes: n(d, "file_size"),
            colours: d["colors"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|c| c.as_str().map(str::to_string))
                .collect(),
        })
        .collect();
    (items, more)
}

// Bing marks the images it offers as wallpapers (`wp`); the others aren't Darwan's to set.
pub fn parse_bing(text: &str) -> Vec<Found> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    v["images"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["wp"].as_bool() == Some(true))
        .map(|i| {
            let base = format!("https://www.bing.com{}", s(i, "urlbase"));
            let copyright = s(i, "copyright");
            let (what, holder) = match copyright.rsplit_once(" (©") {
                Some((w, h)) => (w.to_string(), format!("©{}", h.trim_end_matches(')'))),
                None => (copyright.clone(), String::new()),
            };
            Found {
                source: Source::Bing,
                id: s(i, "hsh"),
                title: if s(i, "title").is_empty() {
                    what.clone()
                } else {
                    format!("{}: {what}", s(i, "title"))
                },
                author: holder.clone(),
                licence: format!("{holder}. Bing image of the day, for use as a wallpaper only"),
                licence_url: s(i, "copyrightlink"),
                page: s(i, "copyrightlink"),
                full: format!("{base}_UHD.jpg"),
                thumb: format!("{base}_800x480.jpg"),
                width: 3840,
                height: 2160,
                bytes: 0,
                colours: Vec::new(),
            }
        })
        .collect()
}

fn strip_html(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

// Wide enough for a desktop and landscape-shaped (no panoramas or portraits), in a still format, with no refused
// word in its categories or title.
pub fn parse_commons(text: &str, allowed: &Allowed) -> Vec<Found> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    v["query"]["pages"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let ii = p["imageinfo"].get(0)?;
            let (w, h) = (n(ii, "width") as u32, n(ii, "height") as u32);
            let ratio = f64::from(w) / f64::from(h.max(1));
            if w < 1920
                || !(1.3..=2.5).contains(&ratio)
                || !matches!(s(ii, "mime").as_str(), "image/jpeg" | "image/png")
            {
                return None;
            }
            let cats: Vec<String> = p["categories"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|c| s(c, "title"))
                .collect();
            let meta = &ii["extmetadata"];
            let m = |k: &str| strip_html(&s(&meta[k], "value"));
            let title = if m("ObjectName").is_empty() {
                s(p, "title").trim_start_matches("File:").to_string()
            } else {
                m("ObjectName")
            };
            if filter::refusal(&format!("{title} {}", cats.join(" ")), allowed).is_some() {
                return None;
            }
            let thumb = s(ii, "thumburl");
            // Commons serves standard widths only; 3840 is one, and a smaller original is used as it is.
            let full = if w > 3840 {
                thumb.replacen("/500px-", "/3840px-", 1)
            } else {
                s(ii, "url")
            };
            Some(Found {
                source: Source::Commons,
                id: n(p, "pageid").to_string(),
                title,
                author: m("Artist"),
                licence: m("LicenseShortName"),
                licence_url: m("LicenseUrl"),
                page: s(ii, "descriptionurl"),
                full,
                thumb,
                width: w,
                height: h,
                bytes: n(ii, "size"),
                colours: Vec::new(),
            })
        })
        .collect()
}

const WALLS_TREE: &str = "https://api.github.com/repos/dharmx/walls/git/trees/main?recursive=1";
const WALLS_PAGE: usize = 24;

fn wallhaven_skips(q: &Query) -> bool {
    filter::is_anime(&q.text) || q.topic.as_deref().is_some_and(filter::is_anime)
}

// The repository's pictures, minus folders the user's switches hide. The folders are the author's; there is no
// word check on this source.
pub fn parse_walls(text: &str, allowed: &Allowed) -> Vec<Found> {
    let Ok(v) = serde_json::from_str::<Value>(text) else {
        return Vec::new();
    };
    v["tree"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| s(e, "type") == "blob" && n(e, "size") <= 20 * 1024 * 1024)
        .filter_map(|e| {
            let path = s(e, "path");
            let (folder, _) = path.split_once('/')?;
            let (stem, ext) = path.rsplit('/').next()?.rsplit_once('.')?;
            let gated = match folder.to_lowercase().as_str() {
                "anime" | "manga" | "evangelion" | "boccha" => !allowed.has("anime"),
                "girl" => !allowed.has("people"),
                _ => false,
            };
            if folder.starts_with('.')
                || gated
                || !["jpg", "jpeg", "png", "webp"].contains(&ext.to_lowercase().as_str())
            {
                return None;
            }
            let url = path.split('/').map(encode).collect::<Vec<_>>().join("/");
            let raw = format!("https://raw.githubusercontent.com/dharmx/walls/main/{url}");
            Some(Found {
                source: Source::Walls,
                id: path.clone(),
                title: stem.replace(['_', '-'], " "),
                author: "dharmx".into(),
                licence: "Collected by dharmx; pictures belong to their owners".into(),
                licence_url: "https://github.com/dharmx/walls".into(),
                page: format!("https://github.com/dharmx/walls/blob/main/{url}"),
                full: raw.clone(),
                thumb: raw,
                width: 0,
                height: 0,
                bytes: n(e, "size"),
                colours: Vec::new(),
            })
        })
        .collect()
}

fn query_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

// "mountains" finds the folder "mountain": plurals are folded before comparing.
fn walls_match(path: &str, words: &[String]) -> bool {
    let fold = |w: &str| w.strip_suffix('s').unwrap_or(w).to_string();
    let hay: Vec<String> = query_words(
        &path
            .rsplit_once('.')
            .map_or(path, |p| p.0)
            .replace('/', " "),
    )
    .iter()
    .map(|w| fold(w))
    .collect();
    words.iter().all(|w| hay.contains(&fold(w)))
}

// There are no dates to sort by: a shuffle seeded by the day, so paging within a day never repeats a picture.
fn walls_page(mut items: Vec<Found>, day: u64, salt: &str, page: u32) -> Page {
    items.sort_by_key(|f| crate::media::fnv1a(format!("{day}{salt}{}", f.id).as_bytes()));
    let start = (page.max(1) as usize - 1) * WALLS_PAGE;
    let more = items.len() > start + WALLS_PAGE;
    Page {
        items: items.into_iter().skip(start).take(WALLS_PAGE).collect(),
        refused: None,
        more,
    }
}

fn shrink(original: &Path, out: &Path) -> Result<(), String> {
    let picture = image::open(original).map_err(|e| format!("{}: {e}", original.display()))?;
    let picture = if picture.width() > 512 {
        picture.resize(512, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        picture
    };
    let dir = out.parent().ok_or("no folder")?;
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
    {
        let mut w = std::io::BufWriter::new(tmp.as_file());
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut w, 85)
            .encode_image(&picture.into_rgb8())
            .map_err(|e| e.to_string())?;
    }
    tmp.persist(out).map_err(|e| e.to_string())?;
    Ok(())
}

fn credit_file(credits: &Path, file: &Path) -> PathBuf {
    credits.join(format!(
        "{:016x}.json",
        crate::media::fnv1a(file.as_os_str().as_encoded_bytes())
    ))
}

fn save_credit(credits: &Path, file: &Path, found: &Found) {
    if std::fs::create_dir_all(credits).is_ok()
        && let Ok(text) = serde_json::to_string(found)
    {
        let _ = std::fs::write(credit_file(credits, file), text);
    }
}

// Where a downloaded wallpaper came from, for the Library's credit line.
pub fn credit(credits: &Path, file: &Path) -> Option<Found> {
    serde_json::from_str(&std::fs::read_to_string(credit_file(credits, file)).ok()?).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none() -> Allowed {
        Allowed::default()
    }

    #[test]
    fn wallhaven_results_are_sfw_only_and_anime_never_passes() {
        let text = r##"{"data":[
            {"id":"a1","purity":"sfw","category":"general","path":"https://w.wallhaven.cc/full/a1/wallhaven-a1.jpg","thumbs":{"large":"https://th/a1.jpg"},"dimension_x":3840,"dimension_y":2160,"file_size":5,"url":"https://wallhaven.cc/w/a1","colors":["#424153"]},
            {"id":"b2","purity":"sketchy","category":"general","path":"x","thumbs":{"large":"y"}},
            {"id":"c3","purity":"nsfw","category":"people","path":"x","thumbs":{"large":"y"}},
            {"id":"d4","purity":"sfw","category":"anime","path":"x","thumbs":{"large":"y"}},
            {"id":"e5","purity":"sfw","category":"people","path":"x","thumbs":{"large":"y"}}
        ],"meta":{"current_page":1,"last_page":4}}"##;
        let ids = |a: &Allowed| {
            parse_wallhaven_search(text, a)
                .0
                .into_iter()
                .map(|f| f.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&none()), ["a1"]);
        let everything = Allowed(filter::GROUPS.iter().map(|g| g.id.to_string()).collect());
        assert_eq!(
            ids(&everything),
            ["a1", "e5"],
            "people follow their switch; anime is dropped even with its switch on"
        );
        let (items, more) = parse_wallhaven_search(text, &none());
        assert!(more);
        assert_eq!(items[0].file_name(), "wallhaven-a1.jpg");
        assert_eq!(items[0].colours, ["#424153"]);
    }

    #[test]
    fn wallhaven_sends_no_request_for_an_anime_search() {
        let q = |text: &str, topic: Option<&str>| Query {
            text: text.into(),
            topic: topic.map(str::to_string),
            ..Query::default()
        };
        assert!(wallhaven_skips(&q("anime wallpaper", None)));
        assert!(wallhaven_skips(&q("", Some("manga"))));
        assert!(!wallhaven_skips(&q("mountain lake", Some("nature"))));
    }

    fn tree(entries: &[(&str, u64)]) -> String {
        let blobs: Vec<String> = entries
            .iter()
            .map(|(p, s)| format!(r#"{{"path":"{p}","type":"blob","size":{s}}}"#))
            .collect();
        format!(
            r#"{{"tree":[{{"path":"anime","type":"tree"}},{}],"truncated":false}}"#,
            blobs.join(",")
        )
    }

    #[test]
    fn walls_folders_follow_the_switches_and_only_pictures_in_folders_are_kept() {
        let text = tree(&[
            ("README.md", 10),
            (".github/banner.png", 10),
            ("top.jpg", 10),
            ("nature/Lake_view-1.JPG", 100),
            ("nature/clip.mp4", 100),
            ("nature/loop.gif", 100),
            ("nature/notes.md", 100),
            ("nature/huge.png", 20 * 1024 * 1024 + 1),
            ("nature/big one.webp", 20 * 1024 * 1024),
            ("anime/a.png", 100),
            ("manga/m.png", 100),
            ("evangelion/e.png", 100),
            ("boccha/b.png", 100),
            ("girl/g.png", 100),
        ]);
        let ids = |a: &Allowed| {
            parse_walls(&text, a)
                .into_iter()
                .map(|f| f.id)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ids(&none()),
            ["nature/Lake_view-1.JPG", "nature/big one.webp"]
        );
        let anime = ids(&Allowed(vec!["anime".into()]));
        assert!(
            [
                "anime/a.png",
                "manga/m.png",
                "evangelion/e.png",
                "boccha/b.png"
            ]
            .iter()
            .all(|p| anime.iter().any(|a| a == p))
                && !anime.contains(&"girl/g.png".to_string()),
            "the anime switch opens the anime folders only: {anime:?}"
        );
        assert!(ids(&Allowed(vec!["people".into()])).contains(&"girl/g.png".to_string()));

        let f = &parse_walls(&text, &none())[1];
        assert_eq!(
            f.full, "https://raw.githubusercontent.com/dharmx/walls/main/nature/big%20one.webp",
            "a space in a path would break the download"
        );
        assert_eq!(f.title, "big one");
        assert_eq!(f.bytes, 20 * 1024 * 1024);
        let lake = &parse_walls(&text, &none())[0];
        assert_eq!(lake.title, "Lake view 1");
        assert_eq!(
            lake.page,
            "https://github.com/dharmx/walls/blob/main/nature/Lake_view-1.JPG"
        );
    }

    #[test]
    fn walls_search_matches_folder_and_name_words_and_forgives_plurals() {
        let words = |s: &str| query_words(s);
        assert!(walls_match("mountain/peak_one.jpg", &words("mountains")));
        assert!(walls_match("mountain/peak_one.jpg", &words("PEAK")));
        assert!(!walls_match(
            "mountain/peak_one.jpg",
            &words("mountains lake")
        ));
        assert!(walls_match("mountain/peak_one.jpg", &words("")));
    }

    #[test]
    fn walls_pages_keep_one_order_through_the_day_without_repeats() {
        let entries: Vec<(String, u64)> =
            (0..60).map(|i| (format!("nature/p{i}.jpg"), 1)).collect();
        let refs: Vec<(&str, u64)> = entries.iter().map(|(p, s)| (p.as_str(), *s)).collect();
        let items = parse_walls(&tree(&refs), &none());
        let first = walls_page(items.clone(), 20000, "day", 1);
        let second = walls_page(items.clone(), 20000, "day", 2);
        let third = walls_page(items.clone(), 20000, "day", 3);
        assert_eq!(
            (first.items.len(), second.items.len(), third.items.len()),
            (24, 24, 12)
        );
        assert!(first.more && second.more && !third.more);
        let all: std::collections::HashSet<&str> = [&first, &second, &third]
            .iter()
            .flat_map(|p| p.items.iter().map(|f| f.id.as_str()))
            .collect();
        assert_eq!(all.len(), 60, "no picture twice across pages");
        assert_eq!(
            walls_page(items.clone(), 20000, "day", 1).items,
            first.items,
            "the same day gives the same order"
        );
        assert_ne!(
            walls_page(items.clone(), 20001, "day", 1).items,
            first.items,
            "a new day shuffles again"
        );
        assert_ne!(
            walls_page(items, 20000, "random", 1).items,
            first.items,
            "random is its own order"
        );
    }

    #[test]
    fn bing_keeps_only_what_it_offers_as_wallpaper_with_its_credit() {
        let text = r#"{"images":[
            {"startdate":"20260929","urlbase":"/th?id=OHR.KasilofRiver_EN-US0047556055","copyright":"The blue waters of the Kasilof River, Alaska, USA (© jared lloyd/Getty Images)","copyrightlink":"https://www.bing.com/search?q=Kasilof","title":"Born of glaciers","wp":true,"hsh":"6321"},
            {"urlbase":"/th?id=OHR.Other","copyright":"x (© y)","title":"t","wp":false,"hsh":"77"}
        ]}"#;
        let items = parse_bing(text);
        assert_eq!(items.len(), 1);
        let f = &items[0];
        assert_eq!(
            f.full,
            "https://www.bing.com/th?id=OHR.KasilofRiver_EN-US0047556055_UHD.jpg"
        );
        assert_eq!(f.author, "© jared lloyd/Getty Images");
        assert!(f.licence.contains("wallpaper only"));
    }

    #[test]
    fn commons_keeps_desktop_shaped_stills_with_their_licence_and_refuses_by_category() {
        let page = |id: u64, w: u64, h: u64, cat: &str| {
            format!(
                r#"{{"pageid":{id},"title":"File:P{id}.jpg","categories":[{{"title":"Category:{cat}"}}],"imageinfo":[{{"width":{w},"height":{h},"size":9,"mime":"image/jpeg","url":"https://upload/P{id}.jpg","thumburl":"https://thumb/P{id}.jpg/500px-P{id}.jpg","descriptionurl":"https://commons/File:P{id}.jpg","extmetadata":{{"Artist":{{"value":"<a href=\"//x\">Ggia</a>"}},"LicenseShortName":{{"value":"CC BY-SA 3.0"}},"LicenseUrl":{{"value":"https://cc/by-sa/3.0"}}}}}}]}}"#
            )
        };
        let text = format!(
            r#"{{"query":{{"pages":[{},{},{},{}]}}}}"#,
            page(1, 6000, 4000, "Mountains of Nepal"),
            page(2, 13000, 1486, "Panoramics"),
            page(3, 1200, 800, "Small"),
            page(4, 4000, 2667, "Nude photography")
        );
        let items = parse_commons(&text, &none());
        assert_eq!(
            items.iter().map(|f| f.title.as_str()).collect::<Vec<_>>(),
            ["P1.jpg"],
            "a panorama, a small one and a refused category are left out"
        );
        let f = &items[0];
        assert_eq!(
            (f.author.as_str(), f.licence.as_str()),
            ("Ggia", "CC BY-SA 3.0"),
            "CC BY-SA needs the author and licence shown"
        );
        assert_eq!(
            f.full, "https://thumb/P1.jpg/3840px-P1.jpg",
            "a standard width, not the 6000 px original"
        );
    }

    #[test]
    fn queries_are_percent_encoded_for_the_apis() {
        assert_eq!(encode("mountain -women"), "mountain%20-women");
    }

    #[test]
    fn credits_are_kept_for_downloaded_files() {
        let d = tempfile::tempdir().unwrap();
        let f = parse_walls(&tree(&[("nature/lake.jpg", 1)]), &none()).remove(0);
        let file = d.path().join("walls-nature-lake.jpg");
        save_credit(&d.path().join("credits"), &file, &f);
        assert_eq!(credit(&d.path().join("credits"), &file), Some(f));
    }
}
