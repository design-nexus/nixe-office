//! Video and audio on slides, and the master slide: objects on every slide.

use super::Present;
use super::model::*;
use crate::window;
use anyhow::{Result, bail};
use gtk::gio;
use gtk::prelude::*;

/// The largest video or audio file a deck takes in.
const MAX_MEDIA: u64 = 512 * 1024 * 1024;

impl Present {
    pub(super) fn pick_media(&self) {
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        let f = gtk::FileFilter::new();
        f.set_name(Some("Video and audio"));
        f.add_mime_type("video/*");
        f.add_mime_type("audio/*");
        filters.append(&f);
        let dialog = gtk::FileDialog::builder().title("Add video or audio").modal(true).filters(&filters).build();
        dialog.set_initial_folder(Some(&gio::File::for_path(crate::paths::home().join("Videos"))));
        let me = self.weak();
        dialog.open(window::window().as_ref(), gio::Cancellable::NONE, move |res| {
            let (Ok(file), Some(p)) = (res, me.upgrade()) else { return };
            if let Err(e) = p.insert_media(&file) {
                window::toast(&format!("Couldn't add that file: {e:#}"));
            }
        });
    }

    fn insert_media(&self, file: &gio::File) -> Result<()> {
        let Some(path) = file.path() else { bail!("it isn't a local file") };
        let size = std::fs::metadata(&path)?.len();
        if size > MAX_MEDIA {
            bail!("it's over {} MB", MAX_MEDIA / 1024 / 1024);
        }
        let bytes = std::fs::read(&path)?;
        let (kind, _) = gio::content_type_guess(Some(&path), &bytes[..bytes.len().min(4096)]);
        let video = kind.starts_with("video/");
        if !video && !kind.starts_with("audio/") {
            bail!("it isn't video or audio");
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or(if video { "mp4" } else { "mp3" }).to_lowercase();
        let name = {
            let media = self.media.borrow();
            let mut n = media.len() + 1;
            while media.contains_key(&format!("clip-{n}.{ext}")) {
                n += 1;
            }
            format!("clip-{n}.{ext}")
        };
        self.media.borrow_mut().insert(name.clone(), bytes);
        let (w, h) = if video { (480.0, 270.0) } else { (320.0, 64.0) };
        self.add_object(Kind::Media { media: name, video }, w, h);
        Ok(())
    }

    /// Edit the master slide (its objects show on every slide), or go back.
    pub(super) fn set_master_mode(&self, on: bool) {
        if self.master_mode.get() == on {
            return;
        }
        self.end_edit();
        self.clear_selection();
        self.master_mode.set(on);
        if let Some(c) = self.controls.borrow().as_ref() {
            c.layout.set_sensitive(!on);
            c.master.set_active(on);
        }
        self.canvas.queue_draw();
        self.sync();
        if on {
            window::toast("Editing the master slide: what you add here shows on every slide.");
        }
    }

    /// A text box with the slide's number, at the bottom right.
    pub(super) fn add_slide_number(&self) {
        self.add_object(Kind::Text { text: SLIDE_NUMBER.into() }, 80.0, 40.0);
        self.end_edit();
        self.change_object(|o| {
            o.x = W - o.w - 24.0;
            o.y = H - o.h - 16.0;
        });
    }
}
