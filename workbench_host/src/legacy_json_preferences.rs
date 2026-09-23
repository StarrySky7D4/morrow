//! Strict conversion of the old shared-preferences snapshot. The original JSON
//! is retained separately by the migration command for forensic comparison.
use crate::{FontPreference, Result, Workbench};
use morrow_workbench_plugin::preferences::{self, Preferences, Source, Track, proto};
use serde_json::{Map, Value};
use std::path::Path;

mod locale_catalog {
    include!("generated_ui_locales.rs");
    pub(super) fn supports(locale: &str) -> bool {
        locale == "system" || UI_LOCALES.contains(&locale)
    }
}

fn object<'a>(value: &'a Value, where_: &str) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .ok_or_else(|| format!("{where_} must be an object").into())
}

fn keys(map: &Map<String, Value>, allowed: &[&str], where_: &str) -> Result<()> {
    for key in map.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unmapped {where_} field: {key}").into());
        }
    }
    Ok(())
}

fn str_field(map: &Map<String, Value>, key: &str, default: &str) -> Result<String> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(default.into()),
        Some(Value::String(value)) => Ok(value.clone()),
        _ => Err(format!("{key} must be text").into()),
    }
}

fn bool_field(map: &Map<String, Value>, key: &str, default: bool) -> Result<bool> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(value)) => Ok(*value),
        _ => Err(format!("{key} must be boolean").into()),
    }
}

fn f64_field(map: &Map<String, Value>, key: &str, default: f64) -> Result<f64> {
    let value = match map.get(key) {
        None | Some(Value::Null) => default,
        Some(Value::Number(value)) => value.as_f64().ok_or(format!("invalid {key}"))?,
        _ => return Err(format!("{key} must be numeric").into()),
    };
    if !value.is_finite() {
        return Err(format!("{key} is not finite").into());
    }
    Ok(value)
}

fn i32_field(map: &Map<String, Value>, key: &str, default: i32) -> Result<i32> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(value)) => {
            Ok(value.as_i64().ok_or(format!("invalid {key}"))?.try_into()?)
        }
        _ => Err(format!("{key} must be an integer").into()),
    }
}

fn u32_field(map: &Map<String, Value>, key: &str, default: u32) -> Result<u32> {
    match map.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Number(value)) => {
            Ok(value.as_u64().ok_or(format!("invalid {key}"))?.try_into()?)
        }
        _ => Err(format!("{key} must be a nonnegative integer").into()),
    }
}

fn optional_u32(map: &Map<String, Value>, key: &str) -> Result<Option<u32>> {
    if map.get(key).is_none_or(Value::is_null) {
        Ok(None)
    } else {
        Ok(Some(u32_field(map, key, 0)?))
    }
}

fn optional_f64(map: &Map<String, Value>, key: &str) -> Result<Option<f64>> {
    if map.get(key).is_none_or(Value::is_null) {
        Ok(None)
    } else {
        Ok(Some(f64_field(map, key, 0.)?))
    }
}

fn source(value: &Value, where_: &str) -> Result<Source> {
    let map = object(value, where_)?;
    keys(map, &["location", "name", "kind", "local"], where_)?;
    for key in ["location", "name", "kind", "local"] {
        if map.get(key).is_none_or(Value::is_null) {
            return Err(format!("{where_}.{key} is required").into());
        }
    }
    let local = bool_field(map, "local", false)?;
    let location = str_field(map, "location", "")?;
    let location = if local {
        url::Url::from_file_path(Path::new(&location))
            .map_err(|_| format!("{where_}.location must be an absolute local path"))?
            .to_string()
    } else {
        location
    };
    Ok(Source {
        location,
        name: str_field(map, "name", "")?,
        kind: str_field(map, "kind", "")?,
        local,
    })
}

fn font(value: Option<&Value>) -> Result<FontPreference> {
    let Some(value) = value.filter(|v| !v.is_null()) else {
        return Ok(FontPreference::default());
    };
    let map = object(value, "uiFont")?;
    keys(map, &["family", "asset", "name"], "uiFont")?;
    let font = FontPreference {
        family: str_field(map, "family", "")?,
        asset: str_field(map, "asset", "")?,
        name: str_field(map, "name", "")?,
    };
    let valid_text = |s: &str, max: usize| s.len() <= max && !s.chars().any(char::is_control);
    if !valid_text(&font.family, 128)
        || font.family.trim() != font.family
        || !valid_text(&font.name, 255)
        || font.asset.is_empty() && !font.name.is_empty()
        || !font.asset.is_empty()
            && (font.asset.len() != 64
                || !font
                    .asset
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                || !font.family.is_empty()
                || font.name.is_empty())
    {
        return Err("invalid uiFont shape".into());
    }
    Ok(font)
}

struct Parsed {
    wire: Vec<u8>,
    locale: String,
    font: FontPreference,
}

fn parse(snapshot: &Value) -> Result<Parsed> {
    let data = object(snapshot, "legacy snapshot")?;
    keys(
        data,
        &[
            "version",
            "ideas",
            "completed",
            "music",
            "uiLocale",
            "uiFont",
            "theme",
            "glass",
            "background",
            "visualStyle",
            "solidTint",
            "frostedOpacity",
            "cornerRadius",
            "rounded",
            "windowRadius",
            "grayscale",
            "themeLightness",
            "customColor",
            "themeColor",
            "liquidCanvas",
            "mediaPlaying",
            "sidebarExpanded",
            "appearanceExpanded",
            "canvasBlur",
            "canvasOpacity",
            "canvasColor",
            "componentCustom",
            "componentBlur",
            "componentOpacity",
            "componentColor",
            "componentMaterials",
            "texture",
        ],
        "legacy snapshot",
    )?;
    if data.get("version").and_then(Value::as_u64) != Some(1) {
        return Err("unsupported legacy snapshot version".into());
    }
    if data
        .get("rounded")
        .is_some_and(|value| !value.is_null() && !value.is_boolean())
    {
        return Err("rounded must be boolean".into());
    }
    let locale = str_field(data, "uiLocale", "system")?;
    // This is the same list as generated_ui_locales.rs. The host checks it again on save.
    if !locale_catalog::supports(&locale) {
        return Err(format!("unsupported uiLocale: {locale}").into());
    }
    let font = font(data.get("uiFont"))?;
    let mut theme = str_field(data, "theme", "white")?;
    if theme == "mist" {
        theme = "custom".into();
    }
    let opacity = f64_field(data, "frostedOpacity", 0.76)?.clamp(0.2, 1.);
    let corner_radius = if data.get("rounded") == Some(&Value::Bool(false)) {
        0.
    } else {
        f64_field(data, "cornerRadius", 20.)?.clamp(0., 32.)
    };
    let lightness = optional_f64(data, "themeLightness")?;
    let custom_color = optional_u32(data, "customColor")?;
    let theme_color = optional_u32(data, "themeColor")?;
    let canvas_color = optional_u32(data, "canvasColor")?;
    let component_color = optional_u32(data, "componentColor")?;
    let appearance = proto::Appearance {
        theme,
        glass: str_field(data, "glass", "frosted")?,
        background: str_field(data, "background", "ambient")?,
        visual_style: str_field(data, "visualStyle", "flat")?,
        solid_tint: u32_field(data, "solidTint", 0)?.min(3),
        opacity,
        corner_radius,
        window_radius: f64_field(data, "windowRadius", 20.)?.clamp(0., 32.),
        grayscale: f64_field(data, "grayscale", 0.)?.clamp(0., 1.),
        lightness: lightness.unwrap_or(0.).clamp(0., 1.),
        has_lightness: lightness.is_some(),
        custom_color: custom_color.unwrap_or(0),
        has_custom_color: custom_color.is_some(),
        theme_color: theme_color.unwrap_or(0),
        has_theme_color: theme_color.is_some(),
        liquid_canvas: bool_field(data, "liquidCanvas", false)?,
        media_playing: bool_field(data, "mediaPlaying", true)?,
        sidebar_expanded: bool_field(data, "sidebarExpanded", true)?,
        appearance_expanded: bool_field(data, "appearanceExpanded", true)?,
        canvas_blur: f64_field(data, "canvasBlur", 0.)?.clamp(0., 40.),
        canvas_opacity: f64_field(data, "canvasOpacity", 0.)?.clamp(0., 1.),
        canvas_color: canvas_color.unwrap_or(0),
        has_canvas_color: canvas_color.is_some(),
        component_custom: bool_field(data, "componentCustom", false)?,
        component_blur: f64_field(data, "componentBlur", 22.)?.clamp(0., 40.),
        component_opacity: f64_field(data, "componentOpacity", opacity)?.clamp(0., 1.),
        component_color: component_color.unwrap_or(0),
        has_component_color: component_color.is_some(),
        material_version: 1,
    };
    let texture = data
        .get("texture")
        .filter(|v| !v.is_null())
        .map(|v| source(v, "texture"))
        .transpose()?;
    let music = match data.get("music") {
        None | Some(Value::Null) => None,
        Some(value) => Some(object(value, "music")?),
    };
    if let Some(music) = music {
        keys(
            music,
            &["index", "showLyrics", "onlineLyrics", "tracks"],
            "music",
        )?;
    }
    let mut tracks = Vec::new();
    if let Some(value) = music.and_then(|v| v.get("tracks")).filter(|v| !v.is_null()) {
        for (index, value) in value
            .as_array()
            .ok_or("music.tracks must be an array")?
            .iter()
            .enumerate()
        {
            let map = object(value, &format!("music.tracks[{index}]"))?;
            keys(
                map,
                &[
                    "source",
                    "cover",
                    "lyrics",
                    "lyricSource",
                    "trackTitle",
                    "artist",
                    "trackDuration",
                    "metadataRead",
                ],
                "music track",
            )?;
            let track_source = source(
                map.get("source").ok_or("music track has no source")?,
                "music track source",
            )?;
            let cover = map
                .get("cover")
                .filter(|v| !v.is_null())
                .map(|v| source(v, "music track cover"))
                .transpose()?;
            let lyrics = str_field(map, "lyrics", "")?;
            tracks.push(Track {
                source: Some(track_source),
                cover,
                lyric_source: str_field(
                    map,
                    "lyricSource",
                    if lyrics.is_empty() {
                        ""
                    } else {
                        "歌词文件"
                    },
                )?,
                lyrics,
                title: str_field(map, "trackTitle", "")?,
                artist: str_field(map, "artist", "")?,
                duration: f64_field(map, "trackDuration", 0.)?,
                metadata_read: bool_field(map, "metadataRead", false)?,
            });
        }
    }
    let mut components = Vec::new();
    if let Some(value) = data.get("componentMaterials").filter(|v| !v.is_null()) {
        let map = object(value, "componentMaterials")?;
        for (id, value) in map {
            let material = object(value, "component material")?;
            keys(
                material,
                &[
                    "enabled",
                    "blur",
                    "opacity",
                    "color",
                    "mode",
                    "followComponent",
                    "cornerRadius",
                ],
                "component material",
            )?;
            let color = optional_u32(material, "color")?;
            let radius = optional_f64(material, "cornerRadius")?;
            components.push(proto::ComponentMaterial {
                id: id.clone(),
                enabled: bool_field(material, "enabled", false)?,
                blur: f64_field(material, "blur", 22.)?,
                opacity: f64_field(material, "opacity", 0.76)?,
                color: color.unwrap_or(0),
                has_color: color.is_some(),
                mode: str_field(material, "mode", "")?,
                corner_radius: radius.unwrap_or(0.),
                has_corner_radius: radius.is_some(),
                follow_component: str_field(material, "followComponent", "")?,
            });
        }
    }
    let mut completed = Vec::new();
    if let Some(value) = data.get("completed").filter(|v| !v.is_null()) {
        for item in value.as_array().ok_or("completed must be an array")? {
            completed.push(
                item.as_str()
                    .ok_or("completed entry must be text")?
                    .to_owned(),
            );
        }
    }
    let preferences = Preferences {
        version: 1,
        appearance: Some(appearance),
        texture,
        tracks,
        index: match music {
            Some(v) => i32_field(v, "index", 0)?,
            None => 0,
        },
        show_lyrics: match music {
            Some(v) => bool_field(v, "showLyrics", false)?,
            None => false,
        },
        online_lyrics: match music {
            Some(v) => bool_field(v, "onlineLyrics", false)?,
            None => false,
        },
        completed,
        components,
    };
    let wire = preferences::encode_wire(&preferences)
        .map_err(|e| format!("legacy preferences invalid: {e}"))?;
    Ok(Parsed { wire, locale, font })
}

/// Called during default read-only preflight. Unknown fields fail explicitly.
pub fn validate_preferences(snapshot: &Value) -> Result<()> {
    parse(snapshot).map(|_| ())
}

/// Save into a newly created target library. The caller verifies the final
/// target and retains the source and raw JSON on any failure.
pub fn apply_preferences(host: &mut Workbench, snapshot: &Value) -> Result<()> {
    let parsed = parse(snapshot)?;
    let locale_revision = host.read_ui_locale()?.1;
    host.save_ui_locale("legacy-json-ui-locale-v1", locale_revision, &parsed.locale)?;
    let font_revision = host.read_ui_font()?.1;
    host.save_ui_font("legacy-json-ui-font-v1", font_revision, &parsed.font)?;
    host.save_preferences("legacy-json-preferences-v1", parsed.wire)?;
    Ok(())
}

/// Confirm that all typed settings written into the target match the decoded
/// source snapshot, including optional values and playback metadata.
pub fn verify_preferences(host: &Workbench, snapshot: &Value) -> Result<()> {
    let expected = parse(snapshot)?;
    let actual = host
        .read_preferences()?
        .ok_or("target has no studio preferences")?;
    if actual != expected.wire {
        return Err("target studio preferences differ from legacy snapshot".into());
    }
    if host.read_ui_locale()?.0 != expected.locale {
        return Err("target UI locale differs from legacy snapshot".into());
    }
    if host.read_ui_font()?.0 != expected.font {
        return Err("target UI font differs from legacy snapshot".into());
    }
    Ok(())
}
