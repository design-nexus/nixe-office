//! Starter documents.

use super::model::*;
use crate::doc::Template;

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "letter",
        title: "Personal letter",
        summary: "Your address, the date and a friendly letter.",
        icon: "nxo-mail-symbolic",
    },
    Template {
        id: "recipe",
        title: "Recipe",
        summary: "Ingredients, steps and notes for a dish.",
        icon: "nxo-chef-hat-symbolic",
    },
    Template {
        id: "journal",
        title: "Journal",
        summary: "Today, the highlights and what's next.",
        icon: "nxo-notebook-pen-symbolic",
    },
];

fn p(text: &str) -> Block {
    Block::Paragraph(Paragraph::text(Style::Normal, text))
}

fn styled(style: Style, text: &str) -> Block {
    Block::Paragraph(Paragraph::text(style, text))
}

fn item(list: List, text: &str) -> Block {
    Block::Paragraph(Paragraph { list: Some(list), runs: vec![Run::text(text)], ..Default::default() })
}

fn lines(lines: &[&str]) -> Vec<Block> {
    lines.iter().map(|l| p(l)).collect()
}

pub fn build(id: &str) -> Option<Document> {
    let mut blocks = Vec::new();
    match id {
        "letter" => {
            blocks.push(Block::Paragraph(Paragraph {
                align: Align::Right,
                runs: vec![Run::text("[Date]")],
                ..Default::default()
            }));
            blocks.extend(lines(&["[Your name]", "[Street address]", "[City, state, postal code]", ""]));
            blocks.extend(lines(&["[Recipient name]", "[Address]", ""]));
            blocks.push(p("Dear [Name],"));
            blocks.push(p(
                "Write your message here. Start with the reason for your letter, then add the details you want to share.",
            ));
            blocks.push(p("Best wishes,"));
            blocks.push(p("[Your name]"));
        }
        "recipe" => {
            blocks.push(styled(Style::Title, "[Recipe name]"));
            blocks.push(Block::Paragraph(Paragraph {
                runs: vec![
                    Run::styled("Serves: ", |r| r.bold = true),
                    Run::text("[number]    "),
                    Run::styled("Prep: ", |r| r.bold = true),
                    Run::text("[minutes]    "),
                    Run::styled("Cook: ", |r| r.bold = true),
                    Run::text("[minutes]"),
                ],
                ..Default::default()
            }));
            blocks.push(styled(Style::H2, "Ingredients"));
            for _ in 0..3 {
                blocks.push(item(List::Bullet, "[amount] [ingredient]"));
            }
            blocks.push(styled(Style::H2, "Directions"));
            for s in ["[First step]", "[Next step]", "[Final step]"] {
                blocks.push(item(List::Number, s));
            }
            blocks.push(styled(Style::H2, "Notes"));
            blocks.push(p("[Substitutions, storage tips or family notes]"));
        }
        "journal" => {
            blocks.push(styled(Style::Title, "[Today’s date]"));
            blocks.push(Block::Paragraph(Paragraph {
                runs: vec![Run::styled("A place for the moments you want to remember.", |r| r.italic = true)],
                ..Default::default()
            }));
            blocks.push(styled(Style::H2, "Today"));
            blocks.push(p("[What happened today?]"));
            blocks.push(styled(Style::H2, "Highlights"));
            blocks.push(item(List::Bullet, "[A good moment]"));
            blocks.push(item(List::Bullet, "[Something you learned]"));
            blocks.push(styled(Style::H2, "Tomorrow"));
            blocks.push(p("[One thing you want to do next]"));
        }
        _ => return None,
    }
    Some(Document { blocks, ..Default::default() })
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_template_builds() {
        for t in super::TEMPLATES {
            assert!(super::build(t.id).is_some(), "{}", t.id);
        }
    }
}
