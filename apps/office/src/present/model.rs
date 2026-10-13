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
    /// A title over two columns of points.
    #[serde(rename = "two-columns")]
    TwoColumns,
    /// A picture filling most of the slide, a title and caption under it.
    Picture,
}

impl Layout {
    pub const ALL: [Layout; 6] =
        [Layout::Title, Layout::Content, Layout::TwoColumns, Layout::Picture, Layout::Section, Layout::Blank];
    pub fn name(self) -> &'static str {
        match self {
            Layout::Title => "Title",
            Layout::Content => "Title and content",
            Layout::TwoColumns => "Two columns",
            Layout::Picture => "Picture with caption",
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
        Layout::TwoColumns => Some((80.0, 56.0, 800.0, 96.0, 42.0, false)),
        Layout::Picture => Some((80.0, 392.0, 800.0, 64.0, 32.0, false)),
        Layout::Blank => None,
    }
}

pub fn body_frame(l: Layout) -> Option<Frame> {
    match l {
        Layout::Title => Some((90.0, 300.0, 780.0, 100.0, 26.0, true)),
        Layout::Content => Some((86.0, 170.0, 790.0, 310.0, 24.0, false)),
        Layout::Section => Some((90.0, 306.0, 780.0, 90.0, 24.0, true)),
        Layout::TwoColumns => Some((86.0, 170.0, 380.0, 310.0, 22.0, false)),
        Layout::Picture => Some((80.0, 456.0, 800.0, 56.0, 20.0, false)),
        Layout::Blank => None,
    }
}

/// The second column's frame (two columns only).
pub fn body2_frame(l: Layout) -> Option<Frame> {
    (l == Layout::TwoColumns).then_some((494.0, 170.0, 380.0, 310.0, 22.0, false))
}

/// Where the picture goes (picture with caption only): x, y, w, h.
pub fn picture_frame(l: Layout) -> Option<(f64, f64, f64, f64)> {
    (l == Layout::Picture).then_some((80.0, 36.0, 800.0, 340.0))
}

/// A placeholder moved from where its layout puts it: x, y, w, h.
pub type Rect = (f64, f64, f64, f64);

/// Placeholders moved or resized (None: where the layout puts them).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Frames {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<Rect>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body2: Option<Rect>,
}

impl Frames {
    pub fn is_default(&self) -> bool {
        *self == Frames::default()
    }
}

/// Which placeholder of a slide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Holder {
    Title,
    Body,
    Body2,
}

impl Holder {
    pub const ALL: [Holder; 3] = [Holder::Title, Holder::Body, Holder::Body2];
    /// The layout's own frame for it.
    pub fn frame(self, l: Layout) -> Option<Frame> {
        match self {
            Holder::Title => title_frame(l),
            Holder::Body => body_frame(l),
            Holder::Body2 => body2_frame(l),
        }
    }
    pub fn of(self, f: &mut Frames) -> &mut Option<Rect> {
        match self {
            Holder::Title => &mut f.title,
            Holder::Body => &mut f.body,
            Holder::Body2 => &mut f.body2,
        }
    }
    pub fn get(self, f: &Frames) -> Option<Rect> {
        match self {
            Holder::Title => f.title,
            Holder::Body => f.body,
            Holder::Body2 => f.body2,
        }
    }
}

/// A placeholder found at `at` in a file: its own place when that isn't the layout's.
pub fn moved(at: Option<Rect>, layout: Option<Frame>) -> Option<Rect> {
    let (a, b) = (at?, layout?);
    [(a.0, b.0), (a.1, b.1), (a.2, b.2), (a.3, b.3)].iter().any(|(u, v)| (u - v).abs() > 2.0).then_some(a)
}

impl Layout {
    /// The id it's saved under (as in the native file).
    pub fn id(self) -> String {
        serde_json::to_value(self).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
    }

    /// Its body text starts at the top of its frame (points), not centred.
    pub fn body_at_top(self) -> bool {
        matches!(self, Layout::Content | Layout::TwoColumns)
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
    /// The second column (two columns).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub body2: String,
    /// Its placeholders, where moved from the layout's places.
    #[serde(skip_serializing_if = "Frames::is_default")]
    pub frames: Frames,
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
    /// Leave out the master slide's objects here (a title slide, say).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub hide_master: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransitionKind {
    Fade,
    Push,
    Wipe,
    Dissolve,
    /// Objects on both slides move to their new place; the rest fade.
    MagicMove,
    /// The new slide slides in over the old one.
    Cover,
    /// The old slide slides away, showing the new one under it.
    Uncover,
    /// The new slide opens out from the middle.
    Split,
    /// The new slide grows from the middle as it fades in.
    Zoom,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 9] = [
        TransitionKind::Fade,
        TransitionKind::Push,
        TransitionKind::Cover,
        TransitionKind::Uncover,
        TransitionKind::Wipe,
        TransitionKind::Split,
        TransitionKind::Zoom,
        TransitionKind::Dissolve,
        TransitionKind::MagicMove,
    ];
    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Fade => "Fade",
            TransitionKind::Push => "Push",
            TransitionKind::Wipe => "Wipe",
            TransitionKind::Dissolve => "Dissolve",
            TransitionKind::MagicMove => "Magic Move",
            TransitionKind::Cover => "Cover",
            TransitionKind::Uncover => "Uncover",
            TransitionKind::Split => "Split",
            TransitionKind::Zoom => "Zoom",
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

/// One click of a slide's builds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
    pub id: u32,
    /// Going away rather than coming in.
    pub out: bool,
    pub effect: Effect,
}

/// An object that comes in (or goes out) on a click during the slideshow.
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
            body2: String::new(),
            frames: Frames::default(),
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
            hide_master: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Shape {
    Rectangle,
    Ellipse,
    Rounded,
    Triangle,
    Diamond,
    Arrow,
    Star,
    Hexagon,
}

impl Shape {
    pub const ALL: [Shape; 8] =
        [Shape::Rectangle, Shape::Rounded, Shape::Ellipse, Shape::Triangle, Shape::Diamond, Shape::Hexagon, Shape::Star, Shape::Arrow];
    pub fn name(self) -> &'static str {
        match self {
            Shape::Rectangle => "Rectangle",
            Shape::Ellipse => "Ellipse",
            Shape::Rounded => "Rounded rectangle",
            Shape::Triangle => "Triangle",
            Shape::Diamond => "Diamond",
            Shape::Arrow => "Arrow",
            Shape::Star => "Star",
            Shape::Hexagon => "Hexagon",
        }
    }
    /// PowerPoint's preset geometry name.
    pub fn preset(self) -> &'static str {
        match self {
            Shape::Rectangle => "rect",
            Shape::Ellipse => "ellipse",
            Shape::Rounded => "roundRect",
            Shape::Triangle => "triangle",
            Shape::Diamond => "diamond",
            Shape::Arrow => "rightArrow",
            Shape::Star => "star5",
            Shape::Hexagon => "hexagon",
        }
    }
    pub fn from_preset(p: &str) -> Shape {
        Shape::ALL.into_iter().find(|s| s.preset() == p).unwrap_or(Shape::Rectangle)
    }
}

/// A line around an object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Outline {
    /// `#rrggbb`.
    pub color: String,
    /// Points.
    pub width: f64,
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
    /// A line across the middle of its box (turned with the box), drawn in the
    /// object's outline colour and width, with arrowheads at either end.
    Line {
        #[serde(default)]
        arrow_start: bool,
        #[serde(default)]
        arrow_end: bool,
    },
    /// Rows of cells; with `header`, the first row is a heading.
    Table {
        rows: Vec<Vec<String>>,
        #[serde(default)]
        header: bool,
    },
    /// Video or audio, by its file name under `media/`; it plays in the slideshow.
    Media {
        media: String,
        video: bool,
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

/// The height of a line's box: room to pick it up.
pub const LINE_BOX: f64 = 20.0;

/// A line's width when it has no outline of its own, in points.
pub const LINE_WIDTH: f64 = 3.0;

impl Object {
    /// A line's colour and width.
    pub fn line_look(&self) -> (String, f64) {
        match &self.outline {
            Some(l) => (l.color.clone(), l.width.max(0.5)),
            None => ("#202329".into(), LINE_WIDTH),
        }
    }
}

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
    /// Goes away on a click during the slideshow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build_out: Option<Build>,
    /// Degrees clockwise, about the object's centre.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline: Option<Outline>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub shadow: bool,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl Object {
    /// The same thing on two slides, for Magic Move: same kind and content.
    pub fn same_as(&self, o: &Object) -> bool {
        match (&self.kind, &o.kind) {
            (Kind::Shape { shape: a, fill: f }, Kind::Shape { shape: b, fill: g }) => a == b && f == g,
            (a, b) => a == b,
        }
    }

    pub fn new(id: u32, kind: Kind, (x, y, w, h): (f64, f64, f64, f64)) -> Object {
        Object { id, kind, x, y, w, h, group: None, build: None, build_out: None, rotation: 0.0, outline: None, shadow: false }
    }

    /// Keep it on the slide and at least a little visible size.
    pub fn clamp(&mut self) {
        self.w = self.w.clamp(24.0, W);
        self.h = self.h.clamp(16.0, H);
        self.x = self.x.clamp(0.0, W - self.w);
        self.y = self.y.clamp(0.0, H - self.h);
    }
    pub fn contains(&self, x: f64, y: f64) -> bool {
        let (x, y) = self.to_local(x, y);
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }

    pub fn center(&self) -> (f64, f64) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    /// A slide point in the object's own unrotated frame.
    pub fn to_local(&self, x: f64, y: f64) -> (f64, f64) {
        self.turn(x, y, -self.rotation)
    }

    fn turn(&self, x: f64, y: f64, degrees: f64) -> (f64, f64) {
        if degrees == 0.0 {
            return (x, y);
        }
        let (cx, cy) = self.center();
        let (s, c) = degrees.to_radians().sin_cos();
        let (dx, dy) = (x - cx, y - cy);
        (cx + dx * c - dy * s, cy + dx * s + dy * c)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deck {
    pub slides: Vec<Slide>,
    /// Placeholders moved on the master, per layout (by its id): new slides of that
    /// layout start with them.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layouts: BTreeMap<String, Frames>,
    /// The master slide: objects drawn on every slide, under its own. A text
    /// object's `{n}` shows the slide's number.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub master: Vec<Object>,
}

/// A text object's `{n}`: the slide number.
pub const SLIDE_NUMBER: &str = "{n}";
/// Set on master objects' ids when they're drawn on a slide.
pub const MASTER_ID: u32 = 1 << 31;

impl Default for Deck {
    fn default() -> Self {
        Deck { slides: vec![Slide { layout: Layout::Title, ..Default::default() }], master: Vec::new(), layouts: BTreeMap::new() }
    }
}

impl Slide {
    pub fn next_id(&self) -> u32 {
        self.objects.iter().map(|o| o.id).max().unwrap_or(0) + 1
    }

    /// Every build, in click order: objects coming in, and going out.
    pub fn steps(&self) -> Vec<Step> {
        let mut v: Vec<(u32, bool, usize, Step)> = Vec::new();
        for (i, o) in self.objects.iter().enumerate() {
            if let Some(b) = o.build {
                v.push((b.order, false, i, Step { id: o.id, out: false, effect: b.effect }));
            }
            if let Some(b) = o.build_out {
                v.push((b.order, true, i, Step { id: o.id, out: true, effect: b.effect }));
            }
        }
        v.sort_by_key(|(order, out, i, _)| (*order, *out, *i));
        v.into_iter().map(|(_, _, _, s)| s).collect()
    }

    /// What isn't showing once `shown` steps have played: objects still to
    /// come in, and those already gone out.
    pub fn hidden_after(&self, shown: usize) -> Vec<u32> {
        let steps = self.steps();
        let done = &steps[..shown.min(steps.len())];
        self.objects
            .iter()
            .filter(|o| {
                let came = o.build.is_none() || done.iter().any(|s| s.id == o.id && !s.out);
                let gone = done.iter().any(|s| s.id == o.id && s.out);
                !came || gone
            })
            .map(|o| o.id)
            .collect()
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

    /// A placeholder's frame here: the layout's, moved where this slide moved it.
    pub fn holder(&self, h: Holder) -> Option<Frame> {
        let f = h.frame(self.layout)?;
        Some(match h.get(&self.frames) {
            Some((x, y, w, hh)) => (x, y, w, hh, f.4, f.5),
            None => f,
        })
    }
    pub fn title_frame(&self) -> Option<Frame> {
        self.holder(Holder::Title)
    }
    pub fn body_frame(&self) -> Option<Frame> {
        self.holder(Holder::Body)
    }
    pub fn body2_frame(&self) -> Option<Frame> {
        self.holder(Holder::Body2)
    }

    /// All the words on the slide, for find.
    pub fn texts(&self) -> Vec<&str> {
        let mut v = vec![self.title.as_str(), self.body.as_str(), self.body2.as_str(), self.notes.as_str()];
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
    /// Slide `i` as shown: the master's objects under its own (unless it hides
    /// them), and `{n}` in text as the slide's number.
    pub fn composed(&self, i: usize) -> Option<Slide> {
        let mut s = self.slides.get(i)?.clone();
        if !s.hide_master && !self.master.is_empty() {
            let mut objects: Vec<Object> = self
                .master
                .iter()
                .cloned()
                .map(|mut o| {
                    // Kept apart from the slide's own ids; never built in.
                    o.id |= MASTER_ID;
                    o.build = None;
                    o.build_out = None;
                    o
                })
                .collect();
            objects.append(&mut s.objects);
            s.objects = objects;
        }
        let n = (i + 1).to_string();
        for o in &mut s.objects {
            if let Kind::Text { text } = &mut o.kind
                && text.contains(SLIDE_NUMBER)
            {
                *text = text.replace(SLIDE_NUMBER, &n);
            }
        }
        Some(s)
    }

    /// What the editor works on: slide `i`, or (`master`) the master's objects
    /// set on slide `i`'s background.
    pub fn editing(&self, i: usize, master: bool) -> Slide {
        let base = self.slides.get(i).cloned().unwrap_or_default();
        if !master {
            return base;
        }
        Slide {
            layout: Layout::Blank,
            title: String::new(),
            body: String::new(),
            objects: self.master.clone(),
            transition: None,
            hide_master: false,
            ..base
        }
    }

    /// Change what the editor works on (see `editing`).
    pub fn edit<R>(&mut self, i: usize, master: bool, f: impl FnOnce(&mut Slide) -> R) -> Option<R> {
        if master {
            let mut s = self.editing(i, true);
            let r = f(&mut s);
            self.master = s.objects;
            return Some(r);
        }
        self.slides.get_mut(i).map(f)
    }

    /// The files under `media/` the deck uses: pictures, video and audio.
    pub fn image_names(&self) -> Vec<String> {
        self.slides
            .iter()
            .flat_map(|s| s.objects.iter())
            .chain(&self.master)
            .filter_map(|o| match &o.kind {
                Kind::Image { image } => Some(image.clone()),
                Kind::Media { media, .. } => Some(media.clone()),
                _ => None,
            })
            .collect()
    }

    /// A new slide after `i`, looking like it (a title slide is followed by content).
    pub fn add_after(&mut self, i: usize) -> Option<usize> {
        if self.slides.len() >= MAX_SLIDES {
            return None;
        }
        let base = self.slides.get(i).cloned().unwrap_or_default();
        let layout = if base.layout == Layout::Title { Layout::Content } else { base.layout };
        // Its placeholders where the master puts them for this layout.
        let frames = self.layout_frames(layout);
        let s = Slide { layout, title: String::new(), body: String::new(), body2: String::new(), frames, notes: String::new(), objects: Vec::new(), ..base };
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

    /// Takes slide `i` out and puts it back at `to` (the slide sorter's drag).
    pub fn move_to(&mut self, i: usize, to: usize) -> Option<usize> {
        if i >= self.slides.len() || to >= self.slides.len() || i == to {
            return None;
        }
        let s = self.slides.remove(i);
        self.slides.insert(to, s);
        Some(to)
    }

    /// Where a layout's placeholders start, as moved on the master.
    pub fn layout_frames(&self, l: Layout) -> Frames {
        self.layouts.get(&l.id()).copied().unwrap_or_default()
    }

    /// Move a layout's placeholder on the master: every slide of that layout follows.
    pub fn set_layout_frame(&mut self, l: Layout, h: Holder, rect: Option<Rect>) {
        let mut f = self.layout_frames(l);
        *h.of(&mut f) = rect;
        if f.is_default() {
            self.layouts.remove(&l.id());
        } else {
            self.layouts.insert(l.id(), f);
        }
        for s in self.slides.iter_mut().filter(|s| s.layout == l) {
            *h.of(&mut s.frames) = rect;
        }
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
            anyhow::bail!("it isn't a Present file");
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
                body2: String::new(),
                frames: Frames::default(),
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
                hide_master: false,
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
        Ok((Deck { slides, master: Vec::new(), layouts: BTreeMap::new() }, media))
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
    fn turned_objects_are_hit_where_they_show() {
        let mut o = Object::new(1, Kind::Shape { shape: Shape::Rectangle, fill: "#000000".into() }, (100.0, 100.0, 200.0, 20.0));
        assert!(o.contains(290.0, 110.0));
        o.rotation = 90.0;
        // Now it stands upright through its centre (200, 110).
        assert!(!o.contains(290.0, 110.0));
        assert!(o.contains(200.0, 190.0));
        assert_eq!(Shape::from_preset("star5"), Shape::Star);
    }

    #[test]
    fn master_objects_show_on_every_slide() {
        let mut d = Deck::default();
        d.add_after(0);
        d.add_after(1);
        d.edit(0, true, |s| s.objects.push(Object::new(1, Kind::Text { text: "Page {n}".into() }, (800.0, 500.0, 120.0, 30.0))));
        assert_eq!(d.master.len(), 1);
        let text = |s: &Slide| match &s.objects[0].kind {
            Kind::Text { text } => text.clone(),
            _ => String::new(),
        };
        assert_eq!(text(&d.composed(2).unwrap()), "Page 3");
        assert!(d.composed(2).unwrap().objects[0].id & MASTER_ID != 0);
        d.slides[1].hide_master = true;
        assert!(d.composed(1).unwrap().objects.is_empty());
        // The slide itself never holds the master's objects.
        assert!(d.slides[2].objects.is_empty());
        d.slides[0].objects.push(Object::new(1, Kind::Media { media: "clip-1.mp4".into(), video: true }, (0.0, 0.0, 10.0, 10.0)));
        assert_eq!(d.image_names(), ["clip-1.mp4"]);
    }

    #[test]
    fn builds_in_and_out() {
        let mut s = Slide::default();
        let shape = |id| Object::new(id, Kind::Shape { shape: Shape::Rectangle, fill: "#000000".into() }, (0.0, 0.0, 50.0, 50.0));
        let (mut a, mut b, c) = (shape(1), shape(2), shape(3));
        a.build = Some(Build { order: 1, effect: Effect::Fade });
        a.build_out = Some(Build { order: 3, effect: Effect::Fly });
        b.build = Some(Build { order: 2, effect: Effect::Appear });
        s.objects = vec![a, b, c];
        assert_eq!(s.steps().len(), 3);
        assert_eq!(s.hidden_after(0), [1, 2]);
        assert_eq!(s.hidden_after(1), [2]);
        assert!(s.hidden_after(2).is_empty());
        assert_eq!(s.hidden_after(3), [1]);
        // Magic Move matches on content, not place.
        let mut moved = s.objects[2].clone();
        moved.x = 300.0;
        assert!(moved.same_as(&s.objects[2]));
        assert!(!moved.same_as(&Object::new(9, Kind::Text { text: "x".into() }, (0.0, 0.0, 1.0, 1.0))));
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
