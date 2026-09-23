//! Portable appearance, playback, lyric and media import policy. No renderer,
//! decoder, filesystem or network access is available to this guest.
use crate::studio_capnp as wire;
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use sha2::{Digest, Sha256};
pub use wire::{MusicAction, ServiceAction};
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub theme: String,
    pub glass: String,
    pub background: String,
    pub solid_tint: u16,
    pub opacity: f64,
    pub corner_radius: f64,
    pub window_radius: f64,
    pub grayscale: f64,
    pub lightness: f64,
    pub has_lightness: bool,
    pub custom_color: u32,
    pub has_custom_color: bool,
    pub theme_color: u32,
    pub has_theme_color: bool,
    pub liquid_canvas: bool,
    pub media_playing: bool,
    pub sidebar_expanded: bool,
    pub appearance_expanded: bool,
    pub canvas_blur: f64,
    pub canvas_opacity: f64,
    pub canvas_color: u32,
    pub has_canvas_color: bool,
    pub component_custom: bool,
    pub component_blur: f64,
    pub component_opacity: f64,
    pub component_color: u32,
    pub has_component_color: bool,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "white".into(),
            glass: "frosted".into(),
            background: "ambient".into(),
            solid_tint: 0,
            opacity: 0.76,
            corner_radius: 20.,
            window_radius: 20.,
            grayscale: 0.,
            lightness: 0.,
            has_lightness: false,
            custom_color: 0,
            has_custom_color: false,
            theme_color: 0,
            has_theme_color: false,
            liquid_canvas: false,
            media_playing: true,
            sidebar_expanded: true,
            appearance_expanded: true,
            canvas_blur: 0.,
            canvas_opacity: 0.,
            canvas_color: 0,
            has_canvas_color: false,
            component_custom: false,
            component_blur: 22.,
            component_opacity: 0.76,
            component_color: 0,
            has_component_color: false,
        }
    }
}
impl Appearance {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !matches!(self.theme.as_str(), "white" | "custom" | "dark")
            || !matches!(self.glass.as_str(), "frosted" | "clear" | "liquid")
            || !matches!(
                self.background.as_str(),
                "ambient" | "solid" | "texture" | "transparent"
            )
            || self.solid_tint > 3
        {
            return Err("appearance choice");
        }
        for (value, min, max) in [
            (self.opacity, 0.2, 1.),
            (self.canvas_blur, 0., 40.),
            (self.canvas_opacity, 0., 1.),
            (self.component_blur, 0., 40.),
            (self.component_opacity, 0., 1.),
            (self.corner_radius, 0., 32.),
            (self.window_radius, 0., 32.),
            (self.grayscale, 0., 1.),
            (self.lightness, 0., 1.),
        ] {
            if !value.is_finite() || value < min || value > max {
                return Err("appearance range");
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Playback {
    pub ids: Vec<String>,
    pub index: i32,
    pub playing: bool,
    pub blocked: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
}
impl Playback {
    pub fn validate(&self) -> Result<(), &'static str> {
        let mut ids = std::collections::BTreeSet::new();
        if self.ids.len() > 1024
            || self
                .ids
                .iter()
                .any(|id| !crate::valid_id(id) || !ids.insert(id))
            || self.index < 0
            || (!self.ids.is_empty() && self.index as usize >= self.ids.len())
            || self.ids.is_empty() && self.index != 0
        {
            return Err("playlist");
        }
        Ok(())
    }
    pub fn apply(
        mut self,
        action: MusicAction,
        value: i64,
        flag: bool,
    ) -> Result<(Self, &'static str), &'static str> {
        self.validate()?;
        let mut effect = "none";
        match action {
            MusicAction::Restore => {
                self.playing = false;
                self.position_ms = 0;
                self.duration_ms = 0;
                effect = "stop";
            }
            MusicAction::Block => {
                self.blocked = flag;
                if flag {
                    self.playing = false;
                    effect = "pause";
                }
            }
            MusicAction::Toggle => {
                if !self.blocked && !self.ids.is_empty() {
                    self.playing = !self.playing;
                    effect = if self.playing { "play" } else { "pause" };
                }
            }
            MusicAction::Seek => {
                if value < 0 {
                    return Err("negative seek");
                }
                self.position_ms = (value as u64).min(self.duration_ms);
                effect = "seek";
            }
            MusicAction::Select | MusicAction::Next | MusicAction::Previous => {
                if !self.ids.is_empty() {
                    let index = match action {
                        MusicAction::Next => i64::from(self.index) + 1,
                        MusicAction::Previous => i64::from(self.index) - 1,
                        _ => value,
                    };
                    self.index = index.rem_euclid(self.ids.len() as i64) as i32;
                    self.playing = flag && !self.blocked;
                    self.position_ms = 0;
                    self.duration_ms = 0;
                    effect = "open";
                }
            }
            MusicAction::Remove => {
                if value < 0 || value as usize >= self.ids.len() {
                    return Err("playlist removal index");
                }
                let was_current = value == i64::from(self.index);
                self.ids.remove(value as usize);
                if value < i64::from(self.index) || self.index as usize >= self.ids.len() {
                    self.index = (self.index - 1).clamp(0, self.ids.len().saturating_sub(1) as i32);
                }
                if was_current {
                    self.position_ms = 0;
                    self.duration_ms = 0;
                    effect = if self.ids.is_empty() {
                        self.playing = false;
                        "stop"
                    } else {
                        "open"
                    };
                }
            }
        }
        if self.blocked || self.ids.is_empty() {
            self.playing = false;
        }
        self.validate()?;
        Ok((self, effect))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LyricLine {
    pub time_ms: u64,
    pub text: String,
}
pub fn parse_lyrics(input: &str) -> Result<Vec<LyricLine>, &'static str> {
    if input.len() > 49152 {
        return Err("lyric budget");
    }
    let offset_re = regex::Regex::new(r"\[offset:([+-]?\d+)\]").unwrap();
    let stamp = regex::Regex::new(r"\[(\d+):(\d{2})(?:[.:](\d{1,3}))?\]").unwrap();
    let offset = offset_re
        .captures(input)
        .and_then(|c| c[1].parse::<i64>().ok())
        .unwrap_or(0);
    let mut lines = Vec::new();
    for line in input.lines() {
        let matches = stamp.captures_iter(line).collect::<Vec<_>>();
        let Some(last) = matches.last() else {
            continue;
        };
        let text = line[last.get(0).unwrap().end()..].trim();
        for m in matches {
            let minutes = m[1].parse::<i64>().map_err(|_| "lyric time")?;
            let seconds = m[2].parse::<i64>().map_err(|_| "lyric time")?;
            let f = m.get(3).map_or("0", |v| v.as_str());
            let ms =
                f.parse::<i64>().map_err(|_| "lyric fraction")? * 10i64.pow((3 - f.len()) as u32);
            let time = minutes
                .saturating_mul(60000)
                .saturating_add(seconds * 1000)
                .saturating_add(ms)
                .saturating_sub(offset)
                .clamp(0, 1 << 40) as u64;
            lines.push(LyricLine {
                time_ms: time,
                text: text.into(),
            });
            if lines.len() > 1024 {
                return Err("lyric line budget");
            }
        }
    }
    lines.sort_by_key(|v| v.time_ms);
    Ok(lines)
}
#[derive(Clone, Debug, Default)]
pub struct Candidate {
    pub title: String,
    pub artist: String,
    pub duration: f64,
    pub synced: bool,
    pub lyrics: String,
}
pub fn lyric_match(
    candidates: &[Candidate],
    title: &str,
    artist: &str,
    duration: f64,
) -> Result<Option<usize>, &'static str> {
    if candidates.len() > 30
        || !duration.is_finite()
        || candidates.iter().any(|c| !c.duration.is_finite())
    {
        return Err("lyric candidates");
    }
    fn normalized(s: &str) -> String {
        s.to_lowercase()
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect()
    }
    let title = normalized(title);
    let artist = normalized(artist);
    let mut matches = candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            normalized(&c.title) == title
                && (artist.is_empty() || normalized(&c.artist) == artist)
                && (duration <= 0. || (c.duration - duration).abs() <= 2.)
                && !c.lyrics.trim().is_empty()
        })
        .collect::<Vec<_>>();
    if artist.is_empty()
        && matches
            .iter()
            .map(|(_, c)| normalized(&c.artist))
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            > 1
    {
        return Ok(None);
    }
    matches.sort_by_key(|(_, c)| !c.synced);
    Ok(matches.first().map(|(i, _)| *i))
}
pub fn import_policy(
    kind: &str,
    size: u64,
    url: &str,
    attachment: bool,
) -> Result<(), &'static str> {
    if !matches!(kind, "image" | "gif" | "video" | "audio" | "file") {
        return Err("media kind");
    }
    let limit = if attachment {
        200
    } else if matches!(kind, "audio" | "video") {
        150
    } else {
        25
    };
    if size > limit * 1024 * 1024 {
        return Err("media size limit");
    }
    if !url.is_empty() {
        let u = url::Url::parse(url).map_err(|_| "media URL")?;
        if !matches!(u.scheme(), "http" | "https")
            || u.host_str().is_none()
            || !u.username().is_empty()
            || u.password().is_some()
        {
            return Err("media URL");
        }
    }
    Ok(())
}
pub fn digest() -> [u8; 32] {
    Sha256::digest(
        include_str!("../schemas/studio.capnp")
            .replace("\r\n", "\n")
            .as_bytes(),
    )
    .into()
}
fn txt(v: capnp::Result<capnp::text::Reader<'_>>) -> Result<String, &'static str> {
    v.map_err(|_| "text")?
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "utf8")
}
pub(crate) fn read_appearance(r: wire::appearance::Reader<'_>) -> Result<Appearance, &'static str> {
    Ok(Appearance {
        theme: txt(r.get_theme())?,
        glass: txt(r.get_glass())?,
        background: txt(r.get_background())?,
        solid_tint: r.get_solid_tint(),
        opacity: r.get_opacity(),
        corner_radius: r.get_corner_radius(),
        window_radius: r.get_window_radius(),
        grayscale: r.get_grayscale(),
        lightness: r.get_lightness(),
        has_lightness: r.get_has_lightness(),
        custom_color: r.get_custom_color(),
        has_custom_color: r.get_has_custom_color(),
        theme_color: r.get_theme_color(),
        has_theme_color: r.get_has_theme_color(),
        liquid_canvas: r.get_liquid_canvas(),
        media_playing: r.get_media_playing(),
        sidebar_expanded: r.get_sidebar_expanded(),
        appearance_expanded: r.get_appearance_expanded(),
        canvas_blur: r.get_canvas_blur(),
        canvas_opacity: r.get_canvas_opacity(),
        canvas_color: r.get_canvas_color(),
        has_canvas_color: r.get_has_canvas_color(),
        component_custom: r.get_component_custom(),
        component_blur: r.get_component_blur(),
        component_opacity: r.get_component_opacity(),
        component_color: r.get_component_color(),
        has_component_color: r.get_has_component_color(),
    })
}
pub(crate) fn write_appearance(mut b: wire::appearance::Builder<'_>, v: &Appearance) {
    b.set_theme(v.theme.as_str());
    b.set_glass(v.glass.as_str());
    b.set_background(v.background.as_str());
    b.set_solid_tint(v.solid_tint);
    b.set_opacity(v.opacity);
    b.set_corner_radius(v.corner_radius);
    b.set_window_radius(v.window_radius);
    b.set_grayscale(v.grayscale);
    b.set_lightness(v.lightness);
    b.set_has_lightness(v.has_lightness);
    b.set_custom_color(v.custom_color);
    b.set_has_custom_color(v.has_custom_color);
    b.set_theme_color(v.theme_color);
    b.set_has_theme_color(v.has_theme_color);
    b.set_liquid_canvas(v.liquid_canvas);
    b.set_media_playing(v.media_playing);
    b.set_sidebar_expanded(v.sidebar_expanded);
    b.set_appearance_expanded(v.appearance_expanded);
    b.set_canvas_blur(v.canvas_blur);
    b.set_canvas_opacity(v.canvas_opacity);
    b.set_canvas_color(v.canvas_color);
    b.set_has_canvas_color(v.has_canvas_color);
    b.set_component_custom(v.component_custom);
    b.set_component_blur(v.component_blur);
    b.set_component_opacity(v.component_opacity);
    b.set_component_color(v.component_color);
    b.set_has_component_color(v.has_component_color);
}
fn read_playback(r: wire::playback::Reader<'_>) -> Result<Playback, &'static str> {
    let ids = r.get_ids().map_err(|_| "playlist")?;
    if ids.len() > 1024 {
        return Err("playlist budget");
    }
    Ok(Playback {
        ids: ids.iter().map(txt).collect::<Result<Vec<_>, _>>()?,
        index: r.get_index(),
        playing: r.get_playing(),
        blocked: r.get_blocked(),
        position_ms: r.get_position_ms(),
        duration_ms: r.get_duration_ms(),
    })
}
fn write_playback(mut b: wire::playback::Builder<'_>, v: &Playback) {
    b.set_index(v.index);
    b.set_playing(v.playing);
    b.set_blocked(v.blocked);
    b.set_position_ms(v.position_ms);
    b.set_duration_ms(v.duration_ms);
    let mut ids = b.init_ids(v.ids.len() as u32);
    for (i, id) in v.ids.iter().enumerate() {
        ids.set(i as u32, id.as_str());
    }
}
pub fn process(input: &[u8]) -> Result<Vec<u8>, &'static str> {
    if input.len() > 65536 {
        return Err("message budget");
    }
    let mut cursor = std::io::Cursor::new(input);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(8192),
            nesting_limit: 16,
        },
    )
    .map_err(|_| "service frame")?;
    if cursor.position() != input.len() as u64 {
        return Err("trailing bytes");
    }
    let r = message
        .get_root::<wire::service_request::Reader>()
        .map_err(|_| "service request")?;
    if r.get_version() != 1 || r.get_digest().map_err(|_| "digest")? != digest() {
        return Err("service contract");
    }
    let mut output = Builder::new_default();
    let mut out = output.init_root::<wire::service_response::Builder>();
    out.set_version(1);
    out.set_digest(&digest());
    out.set_match_index(-1);
    match r.get_action().map_err(|_| "service action")? {
        ServiceAction::Appearance => {
            let a = read_appearance(r.get_appearance().map_err(|_| "appearance")?)?;
            a.validate()?;
            write_appearance(out.reborrow().init_appearance(), &a);
        }
        ServiceAction::Playback => {
            let p = read_playback(r.get_playback().map_err(|_| "playback")?)?;
            let (next, effect) = p.apply(
                r.get_music_action().map_err(|_| "music action")?,
                r.get_value(),
                r.get_flag(),
            )?;
            write_playback(out.reborrow().init_playback(), &next);
            out.set_effect(effect);
        }
        ServiceAction::Lyrics => {
            let text = txt(r.get_text())?;
            let lines = parse_lyrics(&text)?;
            let mut dst = out.reborrow().init_lines(lines.len() as u32);
            for (i, line) in lines.iter().enumerate() {
                let mut v = dst.reborrow().get(i as u32);
                v.set_time_ms(line.time_ms);
                v.set_text(line.text.as_str());
            }
        }
        ServiceAction::LyricMatch => {
            let list = r.get_candidates().map_err(|_| "candidates")?;
            if list.len() > 30 {
                return Err("candidate budget");
            }
            let candidates = list
                .iter()
                .map(|v| {
                    Ok(Candidate {
                        title: txt(v.get_title())?,
                        artist: txt(v.get_artist())?,
                        duration: v.get_duration(),
                        synced: v.get_synced(),
                        lyrics: if v.get_has_lyrics() {
                            "present".into()
                        } else {
                            String::new()
                        },
                    })
                })
                .collect::<Result<Vec<_>, &'static str>>()?;
            let index = lyric_match(
                &candidates,
                &txt(r.get_title())?,
                &txt(r.get_artist())?,
                r.get_duration(),
            )?;
            out.set_match_index(index.map_or(-1, |i| i as i32));
        }
        ServiceAction::ImportPolicy => {
            import_policy(
                &txt(r.get_kind())?,
                r.get_size(),
                &txt(r.get_text())?,
                r.get_flag(),
            )?;
            out.set_effect("accepted");
        }
        ServiceAction::Preferences => {
            return Err("preferences uses its registered typed handler");
        }
        ServiceAction::Markdown => {
            let text = txt(r.get_text())?;
            if text.len() > 49152 {
                return Err("Markdown budget");
            }
            out.set_text(text.as_str());
        }
    }
    let bytes = serialize::write_message_to_words(&output);
    if bytes.len() > 65536 {
        return Err("response budget");
    }
    Ok(bytes)
}
