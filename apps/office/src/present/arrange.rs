//! Selecting several objects, grouping them, lining them up, and the snap
//! guides shown while dragging.

use super::Present;
use super::model::*;
use crate::window;
use gtk::prelude::*;

/// How close (in screen pixels) an edge must come to a guide to snap to it.
const SNAP: f64 = 6.0;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Line {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
}

/// A box: x, y, right, bottom.
type Box4 = (f64, f64, f64, f64);

/// The box around some objects.
fn bounds(objs: &[&Object]) -> Box4 {
    objs.iter().fold((f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY), |(x, y, r, b), o| {
        (x.min(o.x), y.min(o.y), r.max(o.x + o.w), b.max(o.y + o.h))
    })
}

impl Present {
    /// Every selected object: the one clicked, the others shift-clicked, and
    /// everything grouped with them.
    pub(super) fn chosen(&self) -> Vec<u32> {
        let slide = self.slide();
        let mut out: Vec<u32> = Vec::new();
        let also = self.also.borrow().clone();
        let picked = self.selected.get().into_iter().chain(also);
        for id in picked {
            for m in slide.group_of(id) {
                if !out.contains(&m) && slide.objects.iter().any(|o| o.id == m) {
                    out.push(m);
                }
            }
        }
        out
    }

    /// Click on an object: select it (and its group). With Shift, add it to the
    /// selection or take it out. Clicking inside the selection keeps it, so the
    /// whole selection can be dragged.
    pub(super) fn pick(&self, id: u32, shift: bool) {
        let chosen = self.chosen();
        if shift {
            let group = self.slide().group_of(id);
            if chosen.contains(&id) {
                self.also.borrow_mut().retain(|x| !group.contains(x));
                if self.selected.get().is_some_and(|s| group.contains(&s)) {
                    let next = self.also.borrow_mut().pop();
                    self.selected.set(next);
                }
            } else if self.selected.get().is_none() {
                self.selected.set(Some(id));
            } else {
                self.also.borrow_mut().push(id);
            }
        } else if chosen.contains(&id) {
            // Keep the selection; the clicked object leads.
            let prev = self.selected.replace(Some(id));
            let mut also = self.also.borrow_mut();
            also.retain(|x| *x != id);
            if let Some(p) = prev.filter(|p| *p != id) {
                also.push(p);
            }
        } else {
            self.selected.set(Some(id));
            self.also.borrow_mut().clear();
        }
    }

    pub(super) fn clear_selection(&self) {
        self.selected.set(None);
        self.also.borrow_mut().clear();
    }

    /// Remember where each selected object starts, before a drag.
    pub(super) fn begin_move(&self) {
        let slide = self.slide();
        let chosen = self.chosen();
        *self.drag_from.borrow_mut() =
            slide.objects.iter().filter(|o| chosen.contains(&o.id)).map(|o| (o.id, o.x, o.y)).collect();
    }

    /// Drag the selection by (dx, dy) slide points from where it started,
    /// snapping its edges and centre to the slide and to other objects.
    pub(super) fn drag_by(&self, dx: f64, dy: f64, snap: bool) {
        let from = self.drag_from.borrow().clone();
        if from.is_empty() {
            return;
        }
        let i = self.current.get();
        let scale = self.frame().2;
        let mut deck = self.deck.borrow_mut();
        let Some(slide) = deck.slides.get_mut(i) else { return };
        let ids: Vec<u32> = from.iter().map(|f| f.0).collect();
        // The moving box at the unsnapped position.
        let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (id, fx, fy) in &from {
            if let Some(o) = slide.objects.iter().find(|o| o.id == *id) {
                x0 = x0.min(fx + dx);
                y0 = y0.min(fy + dy);
                x1 = x1.max(fx + dx + o.w);
                y1 = y1.max(fy + dy + o.h);
            }
        }
        // Keep the whole selection on the slide.
        let (mut dx, mut dy) = (dx, dy);
        let shift_x = (-x0).max(0.0) - (x1 - W).max(0.0);
        let shift_y = (-y0).max(0.0) - (y1 - H).max(0.0);
        dx += shift_x;
        dy += shift_y;
        (x0, x1, y0, y1) = (x0 + shift_x, x1 + shift_x, y0 + shift_y, y1 + shift_y);
        let mut guides = Vec::new();
        if snap {
            let others: Vec<&Object> = slide.objects.iter().filter(|o| !ids.contains(&o.id)).collect();
            let mut xs = vec![0.0, W / 2.0, W];
            let mut ys = vec![0.0, H / 2.0, H];
            for o in &others {
                xs.extend([o.x, o.x + o.w / 2.0, o.x + o.w]);
                ys.extend([o.y, o.y + o.h / 2.0, o.y + o.h]);
            }
            let best = |edges: [f64; 3], cands: &[f64]| -> Option<(f64, f64)> {
                let mut best: Option<(f64, f64)> = None;
                for e in edges {
                    for c in cands {
                        let d = c - e;
                        if d.abs() * scale <= SNAP && best.is_none_or(|(bd, _)| d.abs() < bd.abs()) {
                            best = Some((d, *c));
                        }
                    }
                }
                best
            };
            if let Some((d, at)) = best([x0, (x0 + x1) / 2.0, x1], &xs) {
                dx += d;
                guides.push((true, at));
            }
            if let Some((d, at)) = best([y0, (y0 + y1) / 2.0, y1], &ys) {
                dy += d;
                guides.push((false, at));
            }
        }
        for (id, fx, fy) in &from {
            if let Some(o) = slide.objects.iter_mut().find(|o| o.id == *id) {
                o.x = fx + dx;
                o.y = fy + dy;
                o.clamp();
            }
        }
        drop(deck);
        *self.guides.borrow_mut() = guides;
        self.canvas.queue_draw();
    }

    pub(super) fn end_move(&self) {
        self.drag_from.borrow_mut().clear();
        self.guides.borrow_mut().clear();
        self.canvas.queue_draw();
    }

    /// Arrow keys: nudge the selection, as one undo step each.
    pub(super) fn nudge(&self, dx: f64, dy: f64) {
        let chosen = self.chosen();
        self.change_slide(|s| {
            let objs: Vec<&Object> = s.objects.iter().filter(|o| chosen.contains(&o.id)).collect();
            let (x0, y0, x1, y1) = bounds(&objs);
            let dx = dx.clamp(-x0, W - x1);
            let dy = dy.clamp(-y0, H - y1);
            for o in s.objects.iter_mut().filter(|o| chosen.contains(&o.id)) {
                o.x += dx;
                o.y += dy;
            }
        });
    }

    pub(super) fn delete_chosen(&self) {
        let chosen = self.chosen();
        if chosen.is_empty() {
            return;
        }
        self.end_edit();
        self.change_slide(|s| s.objects.retain(|o| !chosen.contains(&o.id)));
        self.clear_selection();
        self.sync();
    }

    /// Ctrl+G: the selected objects become one group.
    pub(super) fn group_chosen(&self) {
        let chosen = self.chosen();
        if chosen.len() < 2 {
            window::toast("Shift-click two or more objects to group them.");
            return;
        }
        self.change_slide(|s| {
            let g = s.next_group();
            for o in s.objects.iter_mut().filter(|o| chosen.contains(&o.id)) {
                o.group = Some(g);
            }
        });
    }

    /// Ctrl+Shift+G: split the groups in the selection.
    pub(super) fn ungroup_chosen(&self) {
        let chosen = self.chosen();
        if !self.slide().objects.iter().any(|o| chosen.contains(&o.id) && o.group.is_some()) {
            window::toast("The selection isn't grouped.");
            return;
        }
        self.change_slide(|s| {
            for o in s.objects.iter_mut().filter(|o| chosen.contains(&o.id)) {
                o.group = None;
            }
        });
        // Keep everything that was in the groups selected.
        let lead = self.selected.get();
        *self.also.borrow_mut() = chosen.into_iter().filter(|id| Some(*id) != lead).collect();
    }

    /// Line up the selection: one object (or one group) against the slide,
    /// several against the box around them. Groups move as one.
    pub(super) fn align(&self, line: Line) {
        let chosen = self.chosen();
        if chosen.is_empty() {
            return;
        }
        self.change_slide(|s| {
            // Units that move together: each group, and each loose object.
            let mut units: Vec<Vec<u32>> = Vec::new();
            for id in &chosen {
                if units.iter().any(|u| u.contains(id)) {
                    continue;
                }
                units.push(s.group_of(*id).into_iter().filter(|m| chosen.contains(m)).collect());
            }
            let all: Vec<&Object> = s.objects.iter().filter(|o| chosen.contains(&o.id)).collect();
            let target = if units.len() < 2 { (0.0, 0.0, W, H) } else { bounds(&all) };
            let moves: Vec<(Vec<u32>, f64, f64)> = units
                .into_iter()
                .map(|u| {
                    let objs: Vec<&Object> = s.objects.iter().filter(|o| u.contains(&o.id)).collect();
                    let (x0, y0, x1, y1) = bounds(&objs);
                    let (tx0, ty0, tx1, ty1) = target;
                    let (dx, dy) = match line {
                        Line::Left => (tx0 - x0, 0.0),
                        Line::Center => ((tx0 + tx1) / 2.0 - (x0 + x1) / 2.0, 0.0),
                        Line::Right => (tx1 - x1, 0.0),
                        Line::Top => (0.0, ty0 - y0),
                        Line::Middle => (0.0, (ty0 + ty1) / 2.0 - (y0 + y1) / 2.0),
                        Line::Bottom => (0.0, ty1 - y1),
                    };
                    (u, dx, dy)
                })
                .collect();
            for (u, dx, dy) in moves {
                for o in s.objects.iter_mut().filter(|o| u.contains(&o.id)) {
                    o.x += dx;
                    o.y += dy;
                }
            }
        });
    }

    /// Space three or more objects evenly, across or down.
    pub(super) fn distribute(&self, across: bool) {
        let chosen = self.chosen();
        let slide = self.slide();
        let mut units: Vec<Vec<u32>> = Vec::new();
        for id in &chosen {
            if !units.iter().any(|u| u.contains(id)) {
                units.push(slide.group_of(*id).into_iter().filter(|m| chosen.contains(m)).collect());
            }
        }
        if units.len() < 3 {
            window::toast("Select three or more objects to space them evenly.");
            return;
        }
        self.change_slide(|s| {
            let mut boxes: Vec<(Vec<u32>, Box4)> = units
                .into_iter()
                .map(|u| {
                    let b = bounds(&s.objects.iter().filter(|o| u.contains(&o.id)).collect::<Vec<_>>());
                    (u, b)
                })
                .collect();
            boxes.sort_by(|a, b| {
                let (ka, kb) = if across { (a.1.0, b.1.0) } else { (a.1.1, b.1.1) };
                ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
            });
            let size = |b: &Box4| if across { b.2 - b.0 } else { b.3 - b.1 };
            let start = if across { boxes[0].1.0 } else { boxes[0].1.1 };
            let last = boxes.last().map(|b| if across { b.1.2 } else { b.1.3 }).unwrap_or(start);
            let total: f64 = boxes.iter().map(|b| size(&b.1)).sum();
            let gap = (last - start - total) / (boxes.len() - 1) as f64;
            let mut at = start;
            for (u, b) in &boxes {
                let d = at - if across { b.0 } else { b.1 };
                for o in s.objects.iter_mut().filter(|o| u.contains(&o.id)) {
                    if across {
                        o.x += d;
                    } else {
                        o.y += d;
                    }
                }
                at += size(b) + gap;
            }
        });
    }

    /// Bring the selection to the front or send it to the back, keeping its order.
    pub(super) fn restack(&self, front: bool) {
        let chosen = self.chosen();
        self.change_slide(|s| {
            let (mut picked, rest): (Vec<Object>, Vec<Object>) = s.objects.drain(..).partition(|o| chosen.contains(&o.id));
            s.objects = if front {
                rest.into_iter().chain(picked.drain(..)).collect()
            } else {
                picked.into_iter().chain(rest).collect()
            };
        });
    }
}

#[cfg(test)]
mod tests {
    use super::bounds;
    use super::super::model::*;

    #[test]
    fn bounds_cover_every_object() {
        let a = Object::new(1, Kind::Text { text: String::new() }, (10.0, 20.0, 100.0, 50.0));
        let b = Object::new(2, Kind::Text { text: String::new() }, (200.0, 5.0, 40.0, 40.0));
        assert_eq!(bounds(&[&a, &b]), (10.0, 5.0, 240.0, 70.0));
    }
}
