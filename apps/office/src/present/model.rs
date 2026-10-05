//! A presentation: slides on a 960 × 540 canvas (a 16:9 page in points), each
//! with a layout's title and body, speaker notes, and objects placed on top.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const W: f64 = 960.0;
pub const H: f64 = 540.0;
pub const MAX_SLIDES: usize = 500;
pub const MAX_OBJECTS: usize = 60;
pub const DEFAULT_FONT: &str = "Noto Sans";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    /// A big centred title with a subtitle.
    #[default]
    Title,
    /// A title at the top and a body of points.
    Content,
    /// A centred title and one line: section breaks and endings.
    Section,
    /// Nothing but the objects placed on it.
    Blank,
}

impl Layout {
    pub const ALL: [Layout; 4] = [Layout::Title, Layout::Content, Layout::Section, Layout::Blank];
    pub fn name(self) -> &'static str {
        match self {
            Layout::Title => "Title",
            Layout::Content => "Title and content",
            Layout::Section => "Section",
            Layout::Blank => "Blank",
        }
    }
}

/// Where the title and body go, and how big their text is, for a layout:
/// (x, y, w, h, size, centred).
pub type Frame = (f64, f64, f64, f64, f64, bool);

pub fn title_frame(l: Layout) -> Option<Frame> {
    match l {
        Layout::Title => Some((85.0, 170.0, 790.0, 120.0, 54.0, true)),
        Layout::Content => Some((80.0, 56.0, 800.0, 96.0, 42.0, false)),
        Layout::Section => Some((85.0, 182.0, 790.0, 116.0, 50.0, true)),
        Layout::Blank => None,
    }
}

pub fn body_frame(l: Layout) -> Option<Frame> {
    match l {
        Layout::Title => Some((90.0, 300.0, 780.0, 100.0, 26.0, true)),
        Layout::Content => Some((86.0, 170.0, 790.0, 310.0, 24.0, false)),
        Layout::Section => Some((90.0, 306.0, 780.0, 90.0, 24.0, true)),
        Layout::Blank => None,
    }
}

/// Ready-made looks: (name, background, text colour).
pub const LOOKS: &[(&str, &str, &str)] = &[
    ("Light", "#ffffff", "#202536"),
    ("Dark", "#20263a", "#ffffff"),
    ("Blue", "#2f5bd3", "#ffffff"),
    ("Paper", "#f4f1ea", "#2c2a26"),
    ("Forest", "#284b4a", "#ffffff"),
    ("Plum", "#55395a", "#ffffff"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Slide {
    pub layout: Layout,
    pub title: String,
    pub body: String,
    pub notes: String,
    /// `#rrggbb`.
    pub background: String,
    pub color: String,
    pub font: String,
    pub bold: bool,
    pub italic: bool,
    /// Points, when different from the layout's.
    pub title_size: Option<f64>,
    pub body_size: Option<f64>,
    pub objects: Vec<Object>,
    /// How this slide comes in during the slideshow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<Transition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionKind {
    Fade,
    Push,
    Wipe,
    Dissolve,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 4] = [TransitionKind::Fade, TransitionKind::Push, TransitionKind::Wipe, TransitionKind::Dissolve];
    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Fade => "Fade",
            TransitionKind::Push => "Push",
            TransitionKind::Wipe => "Wipe",
            TransitionKind::Dissolve => "Dissolve",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    /// Seconds.
    pub secs: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Effect {
    Appear,
    Fade,
    /// Flies in from the bottom.
    Fly,
}

impl Effect {
    pub const ALL: [Effect; 3] = [Effect::Appear, Effect::Fade, Effect::Fly];
    pub fn name(self) -> &'static str {
        match self {
            Effect::Appear => "Appear",
            Effect::Fade => "Fade in",
            Effect::Fly => "Fly in",
        }
    }
}

/// An object that comes in on a click during the slideshow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Build {
    /// Lower comes first.
    pub order: u32,
    pub effect: Effect,
}

impl Default for Slide {
    fn default() -> Self {
        Slide {
            layout: Layout::Content,
            title: String::new(),
            body: String::new(),
            notes: String::new(),
            background: LOOKS[0].1.into(),
            color: LOOKS[0].2.into(),
            font: DEFAULT_FONT.into(),
            bold: false,
            italic: false,
            title_size: None,
            body_size: None,
            objects: Vec::new(),
            transition: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    Rectangle,
    Ellipse,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    Text {
        text: String,
    },
    /// A picture, by its file name under `media/`.
    Image {
        image: String,
    },
    Shape {
        shape: Shape,
        fill: String,
    },
    /// Rows of cells; with `header`, the first row is a heading.
    Table {
        rows: Vec<Vec<String>>,
        #[serde(default)]
        header: bool,
    },
    /// A chart from its own small table: one label per point, one list of
    /// values per series.
    Chart {
        chart: ChartKind,
        names: Vec<String>,
        labels: Vec<String>,
        series: Vec<Vec<f64>>,
    },
}

pub use crate::sheets::model::ChartKind;

impl Kind {
    pub fn table(rows: usize, cols: usize) -> Kind {
        let mut t = vec![vec![String::new(); cols.max(1)]; rows.max(1)];
        for (c, cell) in t[0].iter_mut().enumerate() {
            *cell = format!("Heading {}", c + 1);
        }
        Kind::Table { rows: t, header: true }
    }
    pub fn sample_chart() -> Kind {
        Kind::Chart {
            chart: ChartKind::Bar,
            names: vec!["2025".into(), "2026".into()],
            labels: vec!["Q1".into(), "Q2".into(), "Q3".into(), "Q4".into()],
            series: vec![vec![12.0, 18.0, 15.0, 22.0], vec![14.0, 21.0, 19.0, 27.0]],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: u32,
    #[serde(flatten)]
    pub kind: Kind,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    /// Objects with the same group move and select together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<Build>,
}

impl Object {
    pub fn new(id: u32, kind: Kind, (x, y, w, h): (f64, f64, f64, f64)) -> Object {
        Object { id, kind, x, y, w, h, group: None, build: None }
    }

    /// Keep it on the slide and at least a little visible size.
    pub fn clamp(&mut self) {
        self.w = self.w.clamp(24.0, W);
        self.h = self.h.clamp(16.0, H);
        self.x = self.x.clamp(0.0, W - self.w);
        self.y = self.y.clamp(0.0, H - self.h);
    }
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deck {
    pub slides: Vec<Slide>,
}

impl Default for Deck {
    fn default() -> Self {
        Deck { slides: vec![Slide { layout: Layout::Title, ..Default::default() }] }
    }
}

impl Slide {
    pub fn next_id(&self) -> u32 {
        self.objects.iter().map(|o| o.id).max().unwrap_or(0) + 1
    }

    /// Built objects in the order they come in, one click each.
    pub fn build_steps(&self) -> Vec<u32> {
        let mut v: Vec<(u32, usize, u32)> =
            self.objects.iter().enumerate().filter_map(|(i, o)| o.build.map(|b| (b.order, i, o.id))).collect();
        v.sort();
        v.into_iter().map(|(_, _, id)| id).collect()
    }

    /// The objects grouped with `id`, itself included.
    pub fn group_of(&self, id: u32) -> Vec<u32> {
        let g = self.objects.iter().find(|o| o.id == id).and_then(|o| o.group);
        match g {
            Some(g) => self.objects.iter().filter(|o| o.group == Some(g)).map(|o| o.id).collect(),
            None => vec![id],
        }
    }

    pub fn next_group(&self) -> u32 {
        self.objects.iter().filter_map(|o| o.group).max().unwrap_or(0) + 1
    }

    /// All the words on the slide, for find.
    pub fn texts(&self) -> Vec<&str> {
        let mut v = vec![self.title.as_str(), self.body.as_str(), self.notes.as_str()];
        for o in &self.objects {
            match &o.kind {
                Kind::Text { text } => v.push(text),
                Kind::Table { rows, .. } => v.extend(rows.iter().flatten().map(String::as_str)),
                _ => {}
            }
        }
        v
    }
}

impl Deck {
    pub fn image_names(&self) -> Vec<String> {
        self.slides
            .iter()
            .flat_map(|s| s.objects.iter())
            .filter_map(|o| if let Kind::Image { image } = &o.kind { Some(image.clone()) } else { None })
            .collect()
    }

    /// A new slide after `i`, looking like it (a title slide is followed by content).
    pub fn add_after(&mut self, i: usize) -> Option<usize> {
        if self.slides.len() >= MAX_SLIDES {
            return None;
        }
        let base = self.slides.get(i).cloned().unwrap_or_default();
        let layout = if base.layout == Layout::Title { Layout::Content } else { base.layout };
        let s = Slide { layout, title: String::new(), body: String::new(), notes: String::new(), objects: Vec::new(), ..base };
        let at = (i + 1).min(self.slides.len());
        self.slides.insert(at, s);
        Some(at)
    }

    pub fn duplicate(&mut self, i: usize) -> Option<usize> {
        if self.slides.len() >= MAX_SLIDES || i >= self.slides.len() {
            return None;
        }
        let s = self.slides[i].clone();
        self.slides.insert(i + 1, s);
        Some(i + 1)
    }

    pub fn remove(&mut self, i: usize) -> bool {
        if self.slides.len() <= 1 || i >= self.slides.len() {
            return false;
        }
        self.slides.remove(i);
        true
    }

    pub fn move_slide(&mut self, i: usize, by: i64) -> Option<usize> {
        let j = i as i64 + by;
        if i >= self.slides.len() || j < 0 || j as usize >= self.slides.len() {
            return None;
        }
        self.slides.swap(i, j as usize);
        Some(j as usize)
    }

    pub fn to_json(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("a deck always serialises")
    }

    pub fn from_json(bytes: &[u8]) -> anyhow::Result<Deck> {
        let mut d: Deck = serde_json::from_slice(bytes)?;
        d.slides.truncate(MAX_SLIDES);
        if d.slides.is_empty() {
            d = Deck::default();
        }
        for s in &mut d.slides {
            s.objects.truncate(MAX_OBJECTS);
            for o in &mut s.objects {
                o.clamp();
            }
        }
        Ok(d)
    }

    /// A `.npresent` file from the Qt version: JSON with pictures inside, base64.
    pub fn from_legacy_json(bytes: &[u8]) -> anyhow::Result<(Deck, BTreeMap<String, Vec<u8>>)> {
        let v: serde_json::Value = serde_json::from_slice(bytes)?;
        if v.get("format").and_then(|f| f.as_str()) != Some("nexus-present") {
            anyhow::bail!("it isn't a Nexus Present file");
        }
        let mut media = BTreeMap::new();
        let mut slides = Vec::new();
        for item in v.get("slides").and_then(|s| s.as_array()).into_iter().flatten().take(MAX_SLIDES) {
            let gs = |k: &str| item.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
            let gi = |k: &str| item.get(k).and_then(|x| x.as_i64()).unwrap_or(0);
            let style = gi("style");
            let (bg, fg) = match style {
                1 => ("#20263a", "#ffffff"),
                2 => ("#2f5bd3", "#ffffff"),
                _ => ("#ffffff", "#202536"),
            };
            let size = |k: &str| (gi(k) > 0).then(|| gi(k) as f64);
            let mut s = Slide {
                layout: match gi("layout") {
                    1 => Layout::Content,
                    2 => Layout::Section,
                    _ => Layout::Title,
                },
                title: gs("title"),
                body: gs("body"),
                notes: gs("notes"),
                background: Some(gs("backgroundColor")).filter(|c| c.starts_with('#')).unwrap_or_else(|| bg.into()),
                color: Some(gs("fontColor")).filter(|c| c.starts_with('#')).unwrap_or_else(|| fg.into()),
                font: Some(gs("fontFamily")).filter(|f| !f.is_empty()).unwrap_or_else(|| DEFAULT_FONT.into()),
                bold: item.get("bold").and_then(|x| x.as_bool()).unwrap_or(false),
                italic: item.get("italic").and_then(|x| x.as_bool()).unwrap_or(false),
                title_size: size("titleSize"),
                body_size: size("bodySize"),
                objects: Vec::new(),
                transition: None,
            };
            for (n, o) in item.get("elements").and_then(|e| e.as_array()).into_iter().flatten().enumerate() {
                let os = |k: &str| o.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
                let of = |k: &str, d: f64| o.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
                let kind = match os("type").as_str() {
                    "text" => Kind::Text { text: os("text") },
                    "shape" => Kind::Shape {
                        shape: if os("shape") == "ellipse" { Shape::Ellipse } else { Shape::Rectangle },
                        fill: Some(os("fill")).filter(|c| c.starts_with('#')).unwrap_or_else(|| "#5273cf".into()),
                    },
                    "image" => {
                        let Some(bytes) = base64(&os("image")) else { continue };
                        let name = format!("picture-{}.png", media.len() + 1);
                        media.insert(name.clone(), bytes);
                        Kind::Image { image: name }
                    }
                    _ => continue,
                };
                let mut obj = Object::new(
                    n as u32 + 1,
                    kind,
                    (of("x", 120.0), of("y", 120.0), of("width", 320.0), of("height", 180.0)),
                );
                obj.clamp();
                s.objects.push(obj);
            }
            slides.push(s);
        }
        if slides.is_empty() {
            anyhow::bail!("the presentation has no slides");
        }
        Ok((Deck { slides }, media))
    }
}

/// Decode base64 (standard alphabet; a `data:` prefix and whitespace are skipped).
pub fn base64(s: &str) -> Option<Vec<u8>> {
    let s = s.split_once(";base64,").map(|(_, b)| b).unwrap_or(s);
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b' ' | b'\n' | b'\r' | b'\t' => continue,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    (!out.is_empty()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_base64() {
        assert_eq!(base64("aGVsbG8=").unwrap(), b"hello");
        assert_eq!(base64("data:image/png;base64,aGk=").unwrap(), b"hi");
        assert!(base64("!!").is_none());
    }

    #[test]
    fn slides_add_move_and_remove() {
        let mut d = Deck::default();
        d.slides[0].background = "#20263a".into();
        let i = d.add_after(0).unwrap();
        assert_eq!(d.slides[i].layout, Layout::Content);
        assert_eq!(d.slides[i].background, "#20263a");
        assert_eq!(d.duplicate(1), Some(2));
        assert_eq!(d.move_slide(2, -2), Some(0));
        assert!(d.remove(0));
        assert_eq!(d.slides.len(), 2);
        let mut one = Deck::default();
        assert!(!one.remove(0));
    }

    #[test]
    fn objects_stay_on_the_slide() {
        let mut o = Object::new(1, Kind::Text { text: "x".into() }, (900.0, -5.0, 200.0, 2.0));
        o.clamp();
        assert_eq!((o.x, o.y, o.w, o.h), (760.0, 0.0, 200.0, 16.0));
    }

    #[test]
    fn json_round_trip() {
        let mut d = Deck::default();
        d.slides[0].title = "Hi".into();
        d.slides[0].objects.push(Object::new(1, Kind::Shape { shape: Shape::Ellipse, fill: "#ff0000".into() }, (10.0, 10.0, 100.0, 50.0)));
        d.slides[0].objects.push(Object::new(2, Kind::Image { image: "a.png".into() }, (10.0, 10.0, 100.0, 50.0)));
        let back = Deck::from_json(&d.to_json()).unwrap();
        assert_eq!(back, d);
        assert_eq!(back.image_names(), vec!["a.png"]);
    }

    #[test]
    fn reads_qt_files() {
        let json = br##"{"format":"nexus-present","version":2,"slides":[{"title":"Trip","body":"Dates","notes":"n","layout":0,"style":1,"fontFamily":"Noto Serif","fontColor":"","backgroundColor":"","bold":true,"italic":false,"titleSize":60,"bodySize":0,"elements":[{"id":"e1","type":"shape","shape":"ellipse","fill":"#00ff00","x":10,"y":10,"width":100,"height":80,"text":""},{"id":"e2","type":"image","image":"aGk=","x":0,"y":0,"width":64,"height":64}]}]}"##;
        let (d, media) = Deck::from_legacy_json(json).unwrap();
        let s = &d.slides[0];
        assert_eq!(s.layout, Layout::Title);
        assert_eq!(s.background, "#20263a");
        assert_eq!(s.font, "Noto Serif");
        assert_eq!(s.title_size, Some(60.0));
        assert!(s.bold);
        assert_eq!(s.objects.len(), 2);
        assert_eq!(media.values().next().unwrap(), b"hi");
    }
}
