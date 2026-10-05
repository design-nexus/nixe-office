//! Starter presentations.

use super::model::*;
use crate::doc::Template;

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "trip-plan",
        title: "Trip plan",
        summary: "Where you're going and what you'll do.",
        icon: "nxo-plane-symbolic",
    },
    Template {
        id: "celebration",
        title: "Celebration",
        summary: "A party, a birthday or a toast.",
        icon: "nxo-party-popper-symbolic",
    },
    Template { id: "story", title: "Story", summary: "Tell a story in a few slides.", icon: "nxo-book-open-symbolic" },
];

fn slide(layout: Layout, title: &str, body: &str, notes: &str, bg: &str, fg: &str) -> Slide {
    Slide {
        layout,
        title: title.into(),
        body: body.into(),
        notes: notes.into(),
        background: bg.into(),
        color: fg.into(),
        ..Default::default()
    }
}

pub fn build(id: &str) -> Option<Deck> {
    let (dark, light, ink) = match id {
        "trip-plan" => ("#23364d", "#ffffff", "#23364d"),
        "celebration" => ("#55395a", "#fff8f3", "#55395a"),
        "story" => ("#284b4a", "#f4f5ef", "#284b4a"),
        _ => return None,
    };
    let slides = match id {
        "trip-plan" => vec![
            slide(
                Layout::Title,
                "[Our trip]",
                "[Destination] · [Dates]",
                "Introduce the plan and who is traveling.",
                dark,
                "#ffffff",
            ),
            slide(
                Layout::Content,
                "The route",
                "• [First stop]\n• [Next stop]\n• [Final stop]",
                "Add travel details here.",
                light,
                ink,
            ),
            slide(
                Layout::Content,
                "What we want to do",
                "• [Activity]\n• [Place to visit]\n• [Food to try]",
                "Add dates, tickets or links in your own notes.",
                light,
                ink,
            ),
            slide(
                Layout::Content,
                "Before we go",
                "• [Booking]\n• [Packing]\n• [Something to remember]",
                "Review bookings and travel documents.",
                "#eaf3f5",
                ink,
            ),
        ],
        "celebration" => vec![
            slide(
                Layout::Title,
                "[Let’s celebrate!]",
                "[Occasion] · [Date]",
                "Welcome everyone and introduce the occasion.",
                dark,
                "#ffffff",
            ),
            slide(
                Layout::Content,
                "The plan",
                "• [Time and place]\n• [Food and drinks]\n• [Activities]",
                "Add practical details for guests.",
                light,
                ink,
            ),
            slide(
                Layout::Content,
                "People and moments",
                "• [Who to thank]\n• [Favorite memory]\n• [A toast or message]",
                "Personalize this slide for the people being celebrated.",
                light,
                ink,
            ),
            slide(Layout::Section, "Thank you!", "[A closing message]", "Close with a warm message.", dark, "#ffffff"),
        ],
        _ => vec![
            slide(
                Layout::Title,
                "[Our story]",
                "[A subtitle or date]",
                "Introduce the story and who it is about.",
                dark,
                "#ffffff",
            ),
            slide(
                Layout::Content,
                "Where it began",
                "[A short memory or opening chapter]",
                "Add the details you want to tell aloud.",
                light,
                ink,
            ),
            slide(
                Layout::Content,
                "A favorite moment",
                "[What happened and why it matters]",
                "Make this personal with a detail people will remember.",
                light,
                ink,
            ),
            slide(
                Layout::Section,
                "What comes next",
                "[A wish, hope or next chapter]",
                "End with a thought for the future.",
                dark,
                "#ffffff",
            ),
        ],
    };
    Some(Deck { slides })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_template_builds() {
        for t in super::TEMPLATES {
            assert_eq!(super::build(t.id).unwrap().slides.len(), 4, "{}", t.id);
        }
    }
}
