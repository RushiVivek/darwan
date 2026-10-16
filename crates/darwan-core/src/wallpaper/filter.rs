// What the online tab may show. Sexual content in any form is refused always, for every source and inside every
// group the user switches on; the other groups are the user's choice. Terms match whole words, case-insensitively,
// against every title, tag, description and category a source returns, and against the user's search text before
// anything is sent. A false positive only hides an image, so the lists lean wide.

// Never shown, no switch.
pub const SEXUAL: &[&str] = &[
    "nude",
    "nudes",
    "nudity",
    "naked",
    "topless",
    "bottomless",
    "nsfw",
    "explicit",
    "erotic",
    "erotica",
    "porn",
    "porno",
    "pornography",
    "sexy",
    "sex",
    "sexual",
    "sensual",
    "seductive",
    "seduction",
    "provocative",
    "lewd",
    "hentai",
    "ecchi",
    "oppai",
    "ahegao",
    "futanari",
    "yuri",
    "yaoi",
    "loli",
    "lolicon",
    "shota",
    "shotacon",
    "fetish",
    "bdsm",
    "bondage",
    "latex",
    "lingerie",
    "underwear",
    "panties",
    "pantsu",
    "bra",
    "bras",
    "bikini",
    "bikinis",
    "swimsuit",
    "swimsuits",
    "swimwear",
    "bathing suit",
    "one-piece swimsuit",
    "cleavage",
    "sideboob",
    "underboob",
    "busty",
    "breasts",
    "breast",
    "boobs",
    "nipples",
    "butt",
    "ass",
    "booty",
    "thighs",
    "thigh-highs",
    "zettai ryouiki",
    "stockings",
    "pantyhose",
    "garter",
    "upskirt",
    "pin-up",
    "pinup",
    "playboy",
    "stripper",
    "striptease",
    "pole dance",
    "onlyfans",
    "bare shoulders",
    "wet clothes",
    "see-through",
    "skimpy",
    "bunny girl",
    "fanservice",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Group {
    pub id: &'static str,
    pub label: &'static str,
    // Provisional: the user decides later which start on; every group starts off for now.
    pub on_by_default: bool,
    pub terms: &'static [&'static str],
}

pub const GROUPS: &[Group] = &[
    Group {
        id: "people",
        label: "People and portraits",
        on_by_default: false,
        terms: &[
            "woman",
            "women",
            "man",
            "men",
            "girl",
            "girls",
            "boy",
            "boys",
            "lady",
            "ladies",
            "female",
            "male",
            "model",
            "models",
            "portrait",
            "face",
            "faces",
            "selfie",
            "cosplay",
            "people",
            "person",
            "couple",
            "looking at viewer",
            "long hair",
            "short hair",
            "celebrity",
            "actress",
            "actor",
            "legs",
            "feet",
            "body",
        ],
    },
    Group {
        id: "anime",
        label: "Anime and manga",
        on_by_default: false,
        terms: &[
            "anime",
            "manga",
            "waifu",
            "anime girls",
            "anime boys",
            "school uniform",
            "maid",
            "vtuber",
            "chibi",
        ],
    },
    Group {
        id: "games",
        label: "Games",
        on_by_default: false,
        terms: &["video games", "video game", "gaming", "game art"],
    },
    Group {
        id: "series",
        label: "Films and TV",
        on_by_default: false,
        terms: &["movies", "movie", "film stills", "tv series", "tv show"],
    },
    Group {
        id: "war",
        label: "War and weapons",
        on_by_default: false,
        terms: &[
            "war", "warfare", "weapon", "weapons", "gun", "guns", "rifle", "pistol", "knife",
            "sword", "soldier", "soldiers", "military", "army", "combat", "battle", "tank",
            "missile",
        ],
    },
    Group {
        id: "gore",
        label: "Gore and violence",
        on_by_default: false,
        terms: &[
            "gore",
            "blood",
            "bloody",
            "corpse",
            "corpses",
            "dead body",
            "execution",
            "murder",
            "wound",
            "injury",
            "torture",
            "mutilation",
            "violence",
        ],
    },
    Group {
        id: "horror",
        label: "Horror",
        on_by_default: false,
        terms: &[
            "horror",
            "creepy",
            "disturbing",
            "skull",
            "skulls",
            "skeleton",
            "zombie",
            "zombies",
            "demon",
            "demons",
            "hanging",
            "suicide",
        ],
    },
];

// The groups the user switched on, by id; anything else is refused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Allowed(pub Vec<String>);

impl Allowed {
    pub fn defaults() -> Self {
        Allowed(
            GROUPS
                .iter()
                .filter(|g| g.on_by_default)
                .map(|g| g.id.to_string())
                .collect(),
        )
    }

    pub fn has(&self, id: &str) -> bool {
        self.0.iter().any(|a| a == id)
    }

    fn refused(&self) -> impl Iterator<Item = &'static Group> + '_ {
        GROUPS.iter().filter(|g| !self.has(g.id))
    }
}

fn words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '-'))
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

// Whole words or whole phrases: "ass" must not hide "grass", "war" must not hide "Warsaw".
fn contains(words: &[String], term: &str) -> bool {
    let t: Vec<String> = term.split_whitespace().map(str::to_string).collect();
    !t.is_empty() && words.windows(t.len()).any(|w| w == t.as_slice())
}

// Why a text is refused: the first matching term, or None when it may be shown.
pub fn refusal(text: &str, allowed: &Allowed) -> Option<&'static str> {
    let w = words(text);
    SEXUAL
        .iter()
        .chain(allowed.refused().flat_map(|g| g.terms.iter()))
        .find(|t| contains(&w, t))
        .copied()
}

// Anime and manga never come from Wallhaven, whatever the switch: its own category is the uploader's call, so the
// query is the only filter, and it can't exclude what the search asks for.
pub fn is_anime(text: &str) -> bool {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|w| matches!(w, "anime" | "animes" | "manga" | "mangas"))
}

// Search text as Wallhaven gets it: letters, digits and spaces only, so `id:`, `like:`, `@user`, `+tag` and `-tag`
// can't reach the query and undo the exclusions appended after it.
pub fn clean_query(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// Terms Wallhaven excludes itself (`-term`); nothing checks a picture's tags afterwards. Its query has a length
// limit, so the terms that must never be dropped come first, then the most common offenders; the rest can't be caught.
pub fn wallhaven_exclusions(allowed: &Allowed) -> String {
    const FIRST: &[&str] = &[
        "anime",
        "manga",
        "anime girls",
        "anime boys",
        "nude",
        "lingerie",
        "bikini",
        "swimwear",
        "underwear",
        "cleavage",
        "sexy",
        "ass",
        "thighs",
        "stockings",
        "ecchi",
        "bare shoulders",
    ];
    let mut terms: Vec<&str> = FIRST.to_vec();
    for g in allowed.refused() {
        terms.extend(g.terms.iter().take(6).filter(|t| !FIRST.contains(t)));
    }
    let mut out = String::new();
    for t in terms {
        let part = if t.contains(' ') {
            format!("-\"{t}\"")
        } else {
            format!("-{t}")
        };
        if out.len() + part.len() + 1 > 400 {
            break;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&part);
    }
    out
}

// Wallhaven's categories bits (general, anime, people): people only when the user allows them, anime never.
pub fn wallhaven_categories(allowed: &Allowed) -> &'static str {
    if allowed.has("people") { "101" } else { "100" }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_groups() -> Allowed {
        Allowed(GROUPS.iter().map(|g| g.id.to_string()).collect())
    }

    #[test]
    fn sexual_terms_are_refused_even_with_every_group_switched_on() {
        let all = all_groups();
        for text in [
            "Beach bikini sunset",
            "anime girl, cleavage",
            "Lingerie model",
            "ECCHI",
            "bare shoulders",
            "pin-up art",
        ] {
            assert!(
                refusal(text, &all).is_some(),
                "{text}: sexual content has no switch"
            );
        }
    }

    #[test]
    fn optional_groups_are_refused_only_while_switched_off() {
        let none = Allowed::default();
        let anime = Allowed(vec!["anime".into()]);
        assert_eq!(refusal("Anime landscape", &none), Some("anime"));
        assert_eq!(
            refusal("Anime landscape", &anime),
            None,
            "the user switched anime on"
        );
        assert_eq!(
            refusal("tank on a battlefield", &anime),
            Some("tank"),
            "war stays off"
        );
        assert!(
            Allowed::defaults().0.is_empty(),
            "every group starts off until the user decides otherwise"
        );
    }

    #[test]
    fn terms_match_whole_words_so_innocent_titles_survive() {
        let none = Allowed::default();
        for text in [
            "Grassland at dawn",
            "Warsaw old town",
            "Bratislava castle",
            "Manhattan skyline",
            "Sexton Peak",
            "Massif",
        ] {
            assert_eq!(refusal(text, &none), None, "{text}");
        }
        assert_eq!(refusal("A man on a hill", &none), Some("man"));
    }

    #[test]
    fn a_search_cannot_smuggle_wallhaven_operators_past_the_exclusions() {
        assert_eq!(
            clean_query("+nude -mountain id:123 @user like:abc"),
            "nude mountain id 123 user like abc"
        );
        assert_eq!(clean_query("  Lake   Bled! "), "Lake Bled");
    }

    #[test]
    fn wallhaven_never_gets_anime_and_people_follow_the_switch() {
        let none = Allowed::default();
        assert_eq!(wallhaven_categories(&none), "100");
        assert_eq!(wallhaven_categories(&Allowed(vec!["people".into()])), "101");
        assert_eq!(
            wallhaven_categories(&all_groups()),
            "101",
            "the anime bit is never set, even with the anime switch on"
        );
        let ex = wallhaven_exclusions(&none);
        assert!(
            ex.starts_with("-anime -manga -\"anime girls\" -\"anime boys\" -nude")
                && ex.len() <= 400,
            "{ex}"
        );
        let all = wallhaven_exclusions(&all_groups());
        assert!(
            all.contains("-anime") && all.contains("-manga"),
            "no switch lets anime or manga through Wallhaven: {all}"
        );
    }

    #[test]
    fn anime_and_manga_words_are_found_whole() {
        assert!(is_anime("Anime wallpaper"));
        assert!(is_anime("shonen-manga, dark"));
        assert!(!is_anime("manganese mine"));
        assert!(!is_anime("animated sunset"));
    }
}
