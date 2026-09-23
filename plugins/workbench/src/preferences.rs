use crate::{services, studio_capnp as wire};
use capnp::{
    message::{Builder, ReaderOptions},
    serialize,
};
use prost::Message;
use prost_reflect::{DescriptorPool, DynamicMessage, ReflectMessage, Value};
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/morrow.studio.v1.rs"));
}
pub use proto::{Preferences, Source, Track};
/// Aggregate settings transport/storage ceiling; each guest call stays 64 KiB.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const GUEST_BYTES: usize = 65536;
fn txt(v: capnp::Result<capnp::text::Reader<'_>>) -> Result<String, &'static str> {
    v.map_err(|_| "text")?
        .to_str()
        .map(str::to_owned)
        .map_err(|_| "utf8")
}
fn appearance(v: &proto::Appearance) -> services::Appearance {
    services::Appearance {
        theme: v.theme.clone(),
        glass: v.glass.clone(),
        background: v.background.clone(),
        visual_style: if v.visual_style.is_empty() {
            "flat".into()
        } else {
            v.visual_style.clone()
        },
        solid_tint: v.solid_tint as u16,
        opacity: v.opacity,
        corner_radius: v.corner_radius,
        window_radius: v.window_radius,
        grayscale: v.grayscale,
        lightness: v.lightness,
        has_lightness: v.has_lightness,
        custom_color: v.custom_color,
        has_custom_color: v.has_custom_color,
        theme_color: v.theme_color,
        has_theme_color: v.has_theme_color,
        liquid_canvas: v.liquid_canvas,
        media_playing: v.media_playing,
        sidebar_expanded: v.sidebar_expanded,
        appearance_expanded: v.appearance_expanded,
        canvas_blur: v.canvas_blur,
        canvas_opacity: v.canvas_opacity,
        canvas_color: v.canvas_color,
        has_canvas_color: v.has_canvas_color,
        component_custom: v.component_custom,
        component_blur: if v.material_version == 0 {
            22.
        } else {
            v.component_blur
        },
        component_opacity: if v.material_version == 0 {
            v.opacity
        } else {
            v.component_opacity
        },
        component_color: v.component_color,
        has_component_color: v.has_component_color,
    }
}
fn source(r: wire::source::Reader<'_>) -> Result<Source, &'static str> {
    Ok(Source {
        location: txt(r.get_location())?,
        name: txt(r.get_name())?,
        kind: txt(r.get_kind())?,
        local: r.get_local(),
    })
}
fn write_source(mut b: wire::source::Builder<'_>, v: &Source) {
    b.set_location(v.location.as_str());
    b.set_name(v.name.as_str());
    b.set_kind(v.kind.as_str());
    b.set_local(v.local);
}
pub fn validate(v: &Preferences) -> Result<(), &'static str> {
    if v.encoded_len() > MAX_BYTES {
        return Err("preferences budget");
    }
    if v.version != 1
        || v.tracks.len() > 512
        || v.completed.len() > 128
        || v.completed.iter().any(|t| t.len() > 2048)
        || v.index < 0
        || !v.tracks.is_empty() && v.index as usize >= v.tracks.len()
        || v.tracks.is_empty() && v.index != 0
    {
        return Err("preferences fields");
    }
    if v.components.len() > 4096 {
        return Err("component budget");
    }
    let mut ids = std::collections::HashSet::new();
    for c in &v.components {
        if c.id.is_empty()
            || c.id.len() > 256
            || !ids.insert(&c.id)
            || !c.blur.is_finite()
            || !(0.0..=40.0).contains(&c.blur)
            || !c.opacity.is_finite()
            || !(0.0..=1.0).contains(&c.opacity)
            || !matches!(c.mode.as_str(), "" | "frosted" | "clear" | "liquid")
            || c.follow_component.len() > 256
            || !c.corner_radius.is_finite()
            || !(0.0..=32.0).contains(&c.corner_radius)
        {
            return Err("component material");
        }
    }
    // The target may be a built-in component with no custom entry. Only links
    // present in this map can continue a chain or complete a cycle.
    let links: std::collections::HashMap<&str, &str> = v
        .components
        .iter()
        .filter(|c| !c.follow_component.is_empty())
        .map(|c| (c.id.as_str(), c.follow_component.as_str()))
        .collect();
    for origin in links.keys() {
        let mut visited = std::collections::HashSet::new();
        let mut current = *origin;
        while let Some(target) = links.get(current) {
            if !visited.insert(current) {
                return Err("component material cycle");
            }
            current = *target;
        }
    }
    let a = v.appearance.as_ref().ok_or("missing appearance")?;
    if a.solid_tint > 3 {
        return Err("tint");
    }
    appearance(a).validate()?;
    let valid_source = |s: &Source| -> Result<(), &'static str> {
        if s.location.len() > 16384
            || s.name.len() > 4096
            || !matches!(
                s.kind.as_str(),
                "image" | "gif" | "video" | "audio" | "file"
            )
        {
            return Err("source fields");
        }
        if s.local {
            let url = url::Url::parse(&s.location).map_err(|_| "source URI")?;
            if url.scheme() != "file" {
                return Err("local source URI");
            }
        } else {
            services::import_policy(&s.kind, 0, &s.location, false)?;
        }
        Ok(())
    };
    if let Some(s) = &v.texture {
        valid_source(s)?;
    }
    for t in &v.tracks {
        valid_source(t.source.as_ref().ok_or("missing track source")?)?;
        if let Some(s) = &t.cover {
            valid_source(s)?;
        }
        if !t.duration.is_finite()
            || t.duration < 0.
            || t.lyrics.len() > 49152
            || t.title.len() > 16384
            || t.artist.len() > 16384
            || t.lyric_source.len() > 4096
        {
            return Err("track fields");
        }
    }
    Ok(())
}
pub fn decode_wire(bytes: &[u8]) -> Result<Preferences, &'static str> {
    if bytes.len() > MAX_BYTES {
        return Err("preferences budget");
    }
    let mut cursor = std::io::Cursor::new(bytes);
    let message = serialize::read_message(
        &mut cursor,
        ReaderOptions {
            traversal_limit_in_words: Some(MAX_BYTES / 8),
            nesting_limit: 16,
        },
    )
    .map_err(|_| "preferences frame")?;
    if cursor.position() != bytes.len() as u64 {
        return Err("trailing preferences");
    }
    let r = message
        .get_root::<wire::preferences::Reader>()
        .map_err(|_| "preferences")?;
    if r.get_digest().map_err(|_| "preferences digest")? != services::digest() {
        return Err("preferences contract");
    }
    let a = services::read_appearance(r.get_appearance().map_err(|_| "appearance")?)?;
    let appearance = proto::Appearance {
        theme: a.theme,
        glass: a.glass,
        background: a.background,
        visual_style: a.visual_style,
        solid_tint: a.solid_tint as u32,
        opacity: a.opacity,
        corner_radius: a.corner_radius,
        window_radius: a.window_radius,
        grayscale: a.grayscale,
        lightness: a.lightness,
        has_lightness: a.has_lightness,
        custom_color: a.custom_color,
        has_custom_color: a.has_custom_color,
        theme_color: a.theme_color,
        has_theme_color: a.has_theme_color,
        liquid_canvas: a.liquid_canvas,
        media_playing: a.media_playing,
        sidebar_expanded: a.sidebar_expanded,
        appearance_expanded: a.appearance_expanded,
        material_version: 1,
        canvas_blur: a.canvas_blur,
        canvas_opacity: a.canvas_opacity,
        canvas_color: a.canvas_color,
        has_canvas_color: a.has_canvas_color,
        component_custom: a.component_custom,
        component_blur: a.component_blur,
        component_opacity: a.component_opacity,
        component_color: a.component_color,
        has_component_color: a.has_component_color,
    };
    let list = r.get_tracks().map_err(|_| "tracks")?;
    if list.len() > 512 {
        return Err("track budget");
    }
    let mut tracks = Vec::new();
    for t in list {
        tracks.push(Track {
            source: Some(source(t.get_source().map_err(|_| "source")?)?),
            cover: if t.get_cover_present() {
                Some(source(t.get_cover().map_err(|_| "cover")?)?)
            } else {
                None
            },
            lyrics: txt(t.get_lyrics())?,
            lyric_source: txt(t.get_lyric_source())?,
            title: txt(t.get_title())?,
            artist: txt(t.get_artist())?,
            duration: t.get_duration(),
            metadata_read: t.get_metadata_read(),
        });
    }
    let completed = r.get_completed().map_err(|_| "completed")?;
    if completed.len() > 128 {
        return Err("completed budget");
    }
    let component_list = r.get_components().map_err(|_| "components")?;
    if component_list.len() > 4096 {
        return Err("component budget");
    }
    let mut components = Vec::new();
    for c in component_list {
        components.push(proto::ComponentMaterial {
            id: txt(c.get_id())?,
            enabled: c.get_enabled(),
            blur: c.get_blur(),
            opacity: c.get_opacity(),
            color: c.get_color(),
            has_color: c.get_has_color(),
            mode: txt(c.get_mode())?,
            corner_radius: c.get_corner_radius(),
            has_corner_radius: c.get_has_corner_radius(),
            follow_component: txt(c.get_follow_component())?,
        });
    }
    let p = Preferences {
        components,
        version: r.get_version().into(),
        appearance: Some(appearance),
        texture: if r.get_texture_present() {
            Some(source(r.get_texture().map_err(|_| "texture")?)?)
        } else {
            None
        },
        tracks,
        index: r.get_index(),
        show_lyrics: r.get_show_lyrics(),
        online_lyrics: r.get_online_lyrics(),
        completed: completed.iter().map(txt).collect::<Result<_, _>>()?,
    };
    validate(&p)?;
    Ok(p)
}
pub fn encode_wire(v: &Preferences) -> Result<Vec<u8>, &'static str> {
    validate(v)?;
    let mut message = Builder::new_default();
    let mut b = message.init_root::<wire::preferences::Builder>();
    b.set_version(1);
    b.set_digest(&services::digest());
    services::write_appearance(
        b.reborrow().init_appearance(),
        &appearance(v.appearance.as_ref().unwrap()),
    );
    b.set_index(v.index);
    b.set_show_lyrics(v.show_lyrics);
    b.set_online_lyrics(v.online_lyrics);
    if let Some(s) = &v.texture {
        b.set_texture_present(true);
        write_source(b.reborrow().init_texture(), s);
    }
    {
        let mut tracks = b.reborrow().init_tracks(v.tracks.len() as u32);
        for (i, t) in v.tracks.iter().enumerate() {
            let mut b = tracks.reborrow().get(i as u32);
            write_source(b.reborrow().init_source(), t.source.as_ref().unwrap());
            if let Some(s) = &t.cover {
                b.set_cover_present(true);
                write_source(b.reborrow().init_cover(), s);
            }
            b.set_lyrics(t.lyrics.as_str());
            b.set_lyric_source(t.lyric_source.as_str());
            b.set_title(t.title.as_str());
            b.set_artist(t.artist.as_str());
            b.set_duration(t.duration);
            b.set_metadata_read(t.metadata_read);
        }
    }
    {
        let mut components = b.reborrow().init_components(v.components.len() as u32);
        for (i, c) in v.components.iter().enumerate() {
            let mut out = components.reborrow().get(i as u32);
            out.set_id(c.id.as_str());
            out.set_enabled(c.enabled);
            out.set_blur(c.blur);
            out.set_opacity(c.opacity);
            out.set_color(c.color);
            out.set_has_color(c.has_color);
            out.set_mode(c.mode.as_str());
            out.set_corner_radius(c.corner_radius);
            out.set_has_corner_radius(c.has_corner_radius);
            out.set_follow_component(c.follow_component.as_str());
        }
    }
    let mut completed = b.init_completed(v.completed.len() as u32);
    for (i, t) in v.completed.iter().enumerate() {
        completed.set(i as u32, t.as_str());
    }
    let bytes = serialize::write_message_to_words(&message);
    if bytes.len() > MAX_BYTES {
        return Err("preferences budget");
    }
    Ok(bytes)
}
fn descriptor() -> prost_reflect::MessageDescriptor {
    DescriptorPool::decode(
        include_bytes!(concat!(env!("OUT_DIR"), "/properties.descriptor.bin")).as_slice(),
    )
    .unwrap()
    .get_message_by_name("morrow.studio.v1.Preferences")
    .unwrap()
}
pub fn decode_persistent(raw: &[u8]) -> Result<Preferences, &'static str> {
    if raw.len() > MAX_BYTES {
        return Err("preferences budget");
    }
    let v = Preferences::decode(raw).map_err(|_| "stored preferences")?;
    validate(&v)?;
    Ok(v)
}
// Merge known fields recursively; future settings and retained track metadata
// survive edits. Track identity is the host's stable source URI.
fn merge(mut old: DynamicMessage, next: DynamicMessage) -> DynamicMessage {
    for field in next.descriptor().fields() {
        let value = next.get_field(&field).into_owned();
        let prior = old.get_field(&field).into_owned();
        let value = match (prior, value) {
            (Value::Message(a), Value::Message(b)) => Value::Message(merge(a, b)),
            (Value::List(a), Value::List(b))
                if field.name() == "tracks" || field.name() == "components" =>
            {
                Value::List(
                    b.into_iter()
                        .map(|v| {
                            if let Value::Message(b) = v {
                                if field.name() == "components" {
                                    let id = b.get_field_by_name("id").unwrap().into_owned();
                                    let prior = a.iter().filter_map(Value::as_message).find(|m| {
                                        m.get_field_by_name("id").unwrap().as_ref() == &id
                                    });
                                    return Value::Message(
                                        prior.map_or(b.clone(), |p| merge(p.clone(), b)),
                                    );
                                }
                                let source = b.get_field_by_name("source").unwrap().into_owned();
                                let id = source
                                    .as_message()
                                    .unwrap()
                                    .get_field_by_name("location")
                                    .unwrap()
                                    .into_owned();
                                let prior = a.iter().filter_map(Value::as_message).find(|m| {
                                    m.get_field_by_name("source")
                                        .unwrap()
                                        .as_message()
                                        .unwrap()
                                        .get_field_by_name("location")
                                        .unwrap()
                                        .as_ref()
                                        == &id
                                });
                                Value::Message(prior.map_or(b.clone(), |p| merge(p.clone(), b)))
                            } else {
                                v
                            }
                        })
                        .collect(),
                )
            }
            (_, v) => v,
        };
        if next.has_field(&field) {
            old.set_field(&field, value);
        } else {
            old.clear_field(&field);
        }
    }
    old
}
pub fn encode_persistent(
    v: &Preferences,
    previous: Option<&[u8]>,
) -> Result<Vec<u8>, &'static str> {
    validate(v)?;
    let next = DynamicMessage::decode(descriptor(), v.encode_to_vec().as_slice())
        .map_err(|_| "preferences")?;
    let result = if let Some(raw) = previous {
        decode_persistent(raw)?;
        merge(
            DynamicMessage::decode(descriptor(), raw).map_err(|_| "preferences")?,
            next,
        )
    } else {
        next
    };
    let bytes = result.encode_to_vec();
    if bytes.len() > MAX_BYTES {
        return Err("preferences budget");
    }
    Ok(bytes)
}

/// Decompose an aggregate into independently bounded validation inputs. The
/// trusted adapter also validates cross-page counts/index and preserves the
/// original aggregate; a guest cannot replace one page with different content.
pub fn validation_pages(p: &Preferences) -> Result<Vec<Vec<u8>>, &'static str> {
    validate(p)?;
    let mut header = p.clone();
    header.tracks.clear();
    header.completed.clear();
    header.components.clear();
    header.index = 0;
    let encode = |p: &Preferences| {
        let bytes = encode_wire(p)?;
        if bytes.len() > GUEST_BYTES {
            return Err("individual preference item exceeds guest budget");
        }
        Ok(bytes)
    };
    let mut pages = vec![encode(&header)?];
    let empty = Preferences {
        version: 1,
        appearance: Some(proto::Appearance {
            theme: "white".into(),
            glass: "frosted".into(),
            background: "ambient".into(),
            opacity: 0.76,
            corner_radius: 20.,
            window_radius: 20.,
            component_blur: 22.,
            component_opacity: 0.76,
            material_version: 1,
            ..Default::default()
        }),
        ..Default::default()
    };
    // One record per page avoids playlist-size-dependent validation cost. A
    // source/cover/lyric record still has its own explicit 64 KiB guest bound.
    for track in &p.tracks {
        let mut page = empty.clone();
        page.tracks.push(track.clone());
        pages.push(encode(&page)?);
    }
    for items in p.completed.chunks(16) {
        let mut page = empty.clone();
        page.completed = items.to_vec();
        pages.push(encode(&page)?);
    }
    for items in p.components.chunks(32) {
        let mut page = empty.clone();
        page.components = items.to_vec();
        pages.push(encode(&page)?);
    }
    Ok(pages)
}
