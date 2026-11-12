//! A small Markdown renderer for the help pages, with links to parameters and topics.
//!
//! A bare parameter code in the text (`P02-10`) is a link to that parameter. `[text](P02-10)`,
//! `[text](topic:filters)` and `[text](https://...)` are links too.

use eframe::egui::{self, RichText};
use jmc_core::Schema;
use jmc_core::docs::{self, Segment, Target};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

use crate::theme;

/// A piece of inline text with one style.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub link: Option<Target>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading(u8, Vec<Span>),
    Para(Vec<Span>),
    /// A list item: nesting depth (0 = top), its mark ("•", "2.", or "" for a second
    /// paragraph of the same item) and its text.
    Item { depth: usize, mark: String, spans: Vec<Span> },
    Table { head: Vec<Vec<Span>>, rows: Vec<Vec<Vec<Span>>> },
    Code(String),
    Rule,
}

#[derive(Default)]
struct TableState {
    head: Vec<Vec<Span>>,
    rows: Vec<Vec<Vec<Span>>>,
    row: Vec<Vec<Span>>,
}

#[derive(Default)]
struct Builder {
    blocks: Vec<Block>,
    spans: Vec<Span>,
    bold: usize,
    italic: usize,
    /// `Some` inside a link: the target, if it is one we know how to follow.
    link: Option<Option<Target>>,
    /// One entry for each open list: the next number, or `None` for bullets.
    lists: Vec<Option<u64>>,
    /// One mark for each open list item; emptied once the item's first text is out.
    items: Vec<String>,
    heading: Option<u8>,
    table: Option<TableState>,
    code: Option<String>,
}

impl Builder {
    fn span(&mut self, text: &str, code: bool, link: Option<Target>) {
        self.spans.push(Span { text: text.to_string(), bold: self.bold > 0, italic: self.italic > 0, code, link });
    }

    fn text(&mut self, text: &str) {
        if let Some(link) = self.link.clone() {
            self.span(text, false, link);
            return;
        }
        for s in docs::segments(text) {
            match s {
                Segment::Text(t) => self.span(t, false, None),
                Segment::Code(c) => self.span(c, false, Some(Target::Param(c.to_string()))),
            }
        }
    }

    /// End the current run of text: a list item inside a list, else a paragraph.
    fn flush(&mut self) {
        if self.spans.is_empty() {
            return;
        }
        let spans = std::mem::take(&mut self.spans);
        let depth = self.items.len().saturating_sub(1);
        match self.items.last_mut() {
            Some(mark) => self.blocks.push(Block::Item { depth, mark: std::mem::take(mark), spans }),
            None => self.blocks.push(Block::Para(spans)),
        }
    }

    fn event(&mut self, e: Event) {
        match e {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(t) => match &mut self.code {
                Some(code) => code.push_str(&t),
                None => self.text(&t),
            },
            Event::Code(t) => {
                let link = match &self.link {
                    Some(l) => l.clone(),
                    None => docs::is_code(&t).then(|| Target::Param(t.to_string())),
                };
                self.span(&t, true, link);
            }
            Event::SoftBreak | Event::HardBreak => self.text(" "),
            Event::Rule => {
                self.flush();
                self.blocks.push(Block::Rule);
            }
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag) {
        match tag {
            Tag::Heading { level, .. } => {
                self.flush();
                self.heading = Some(level as u8);
            }
            Tag::List(first) => {
                // Text of a tight item comes before its nested list.
                self.flush();
                self.lists.push(first);
            }
            Tag::Item => {
                let mark = match self.lists.last_mut() {
                    Some(Some(n)) => {
                        let m = format!("{n}.");
                        *n += 1;
                        m
                    }
                    _ => "•".to_string(),
                };
                self.items.push(mark);
            }
            Tag::Table(_) => {
                self.flush();
                self.table = Some(TableState::default());
            }
            Tag::TableRow => {
                if let Some(t) = &mut self.table {
                    t.row.clear();
                }
            }
            Tag::TableCell => self.spans.clear(),
            Tag::Emphasis => self.italic += 1,
            Tag::Strong => self.bold += 1,
            Tag::Link { dest_url, .. } => self.link = Some(Target::parse(&dest_url)),
            Tag::CodeBlock(_) => {
                self.flush();
                self.code = Some(String::new());
            }
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => self.flush(),
            TagEnd::Heading(_) => {
                let spans = std::mem::take(&mut self.spans);
                self.blocks.push(Block::Heading(self.heading.take().unwrap_or(2), spans));
            }
            TagEnd::List(_) => {
                self.lists.pop();
            }
            TagEnd::Item => {
                self.flush();
                self.items.pop();
            }
            TagEnd::TableCell => {
                let cell = std::mem::take(&mut self.spans);
                if let Some(t) = &mut self.table {
                    t.row.push(cell);
                }
            }
            TagEnd::TableHead => {
                if let Some(t) = &mut self.table {
                    t.head = std::mem::take(&mut t.row);
                }
            }
            TagEnd::TableRow => {
                if let Some(t) = &mut self.table {
                    let row = std::mem::take(&mut t.row);
                    t.rows.push(row);
                }
            }
            TagEnd::Table => {
                if let Some(t) = self.table.take() {
                    self.blocks.push(Block::Table { head: t.head, rows: t.rows });
                }
            }
            TagEnd::Emphasis => self.italic = self.italic.saturating_sub(1),
            TagEnd::Strong => self.bold = self.bold.saturating_sub(1),
            TagEnd::Link => self.link = None,
            TagEnd::CodeBlock => {
                if let Some(code) = self.code.take() {
                    self.blocks.push(Block::Code(code.trim_end().to_string()));
                }
            }
            _ => {}
        }
    }
}

/// Parse Markdown into blocks.
pub fn parse(text: &str) -> Vec<Block> {
    let mut b = Builder::default();
    for e in Parser::new_ext(text, Options::ENABLE_TABLES) {
        b.event(e);
    }
    b.flush();
    b.blocks
}

/// Draw the Markdown `text`. Returns the link the user clicked, if any.
pub fn show(ui: &mut egui::Ui, schema: &Schema, text: &str) -> Option<Target> {
    let mut clicked = None;
    for block in parse(text) {
        match block {
            Block::Heading(level, spans) => {
                ui.add_space(if level <= 2 { 8.0 } else { 4.0 });
                let size = match level {
                    1 => 19.0,
                    2 => 16.5,
                    _ => 15.0,
                };
                inline(ui, schema, &spans, Some(size), &mut clicked);
            }
            Block::Para(spans) => inline(ui, schema, &spans, None, &mut clicked),
            Block::Item { depth, mark, spans } => {
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(12.0 * (depth as f32 + 1.0) - 12.0);
                    ui.add_sized([20.0, 18.0], egui::Label::new(RichText::new(mark).color(theme::ACCENT)));
                    ui.vertical(|ui| inline(ui, schema, &spans, None, &mut clicked));
                });
            }
            Block::Table { head, rows } => table(ui, schema, &head, &rows, &mut clicked),
            Block::Code(code) => {
                egui::Frame::new().fill(theme::CANVAS).corner_radius(6).inner_margin(8).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(code).monospace());
                });
            }
            Block::Rule => {
                ui.separator();
            }
        }
    }
    clicked
}

/// One run of inline text, wrapped to the width.
fn inline(ui: &mut egui::Ui, schema: &Schema, spans: &[Span], size: Option<f32>, clicked: &mut Option<Target>) {
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for s in spans {
            let mut rt = RichText::new(&s.text);
            if let Some(size) = size {
                rt = rt.size(size).strong();
            }
            if s.bold {
                rt = rt.strong();
            }
            if s.italic {
                rt = rt.italics();
            }
            if s.code {
                rt = rt.code();
            }
            match &s.link {
                Some(Target::Param(code)) => match schema.get(code) {
                    Ok(def) => {
                        let r = ui.link(rt.color(theme::ACCENT)).on_hover_text(format!("{}  {}", def.code, def.name));
                        if r.clicked() {
                            *clicked = s.link.clone();
                        }
                    }
                    Err(_) => {
                        ui.label(rt.color(theme::TEXT_DIM)).on_hover_text("Not in the parameter table");
                    }
                },
                Some(t @ Target::Topic(_)) => {
                    match docs::page(&t.key()) {
                        Some(page) => {
                            let r = ui.link(rt.color(theme::ACCENT)).on_hover_text(page.title.clone().unwrap_or_default());
                            if r.clicked() {
                                *clicked = s.link.clone();
                            }
                        }
                        None => {
                            ui.label(rt.color(theme::TEXT_DIM));
                        }
                    }
                }
                Some(Target::Web(url)) => {
                    ui.hyperlink_to(rt.color(theme::ACCENT), url);
                }
                None => {
                    ui.label(rt);
                }
            }
        }
    });
}

fn table(ui: &mut egui::Ui, schema: &Schema, head: &[Vec<Span>], rows: &[Vec<Vec<Span>>], clicked: &mut Option<Target>) {
    let cols = head.len().max(rows.iter().map(Vec::len).max().unwrap_or(0));
    if cols == 0 {
        return;
    }
    // Give each column a share of the width by the length of its longest text.
    let len = |cell: &Vec<Span>| cell.iter().map(|s| s.text.chars().count()).sum::<usize>();
    let weight: Vec<f32> = (0..cols)
        .map(|c| {
            let longest = std::iter::once(head).chain(rows.iter().map(Vec::as_slice)).filter_map(|r| r.get(c)).map(len).max().unwrap_or(1);
            (longest as f32).clamp(6.0, 60.0)
        })
        .collect();
    let total: f32 = weight.iter().sum();
    let spacing = 12.0;
    let usable = (ui.available_width() - spacing * (cols as f32 - 1.0) - 4.0).max(cols as f32 * 50.0);
    let widths: Vec<f32> = weight.iter().map(|w| usable * w / total).collect();
    egui::Grid::new(ui.next_auto_id()).striped(true).spacing([spacing, 6.0]).show(ui, |ui| {
        for (c, w) in widths.iter().enumerate() {
            ui.vertical(|ui| {
                ui.set_width(*w);
                let spans: Vec<Span> = head.get(c).cloned().unwrap_or_default().into_iter().map(|s| Span { bold: true, ..s }).collect();
                inline(ui, schema, &spans, None, clicked);
            });
        }
        ui.end_row();
        for row in rows {
            for (c, w) in widths.iter().enumerate() {
                ui.vertical(|ui| {
                    ui.set_width(*w);
                    if let Some(cell) = row.get(c) {
                        inline(ui, schema, cell, None, clicked);
                    }
                });
            }
            ui.end_row();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(spans: &[Span]) -> String {
        spans.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn blocks_and_links() {
        let md = "# Title\n\nSet P02-10 first, then [the filter](topic:filters).\n\n\
                  1. One `P02-11`\n2. Two\n   - Nested **bold**\n\n\
                  | Code | Use |\n|---|---|\n| P02-00 | gain |\n\n---\n";
        let b = parse(md);
        assert!(matches!(&b[0], Block::Heading(1, s) if texts(s) == "Title"));
        let Block::Para(p) = &b[1] else { panic!("{:?}", b[1]) };
        assert_eq!(texts(p), "Set P02-10 first, then the filter.");
        assert_eq!(p[1].link, Some(Target::Param("P02-10".into())));
        assert_eq!(p[3].link, Some(Target::Topic("filters".into())));
        let Block::Item { depth: 0, mark, spans } = &b[2] else { panic!("{:?}", b[2]) };
        assert_eq!(mark, "1.");
        assert!(spans.iter().any(|s| s.code && s.link == Some(Target::Param("P02-11".into()))));
        assert!(matches!(&b[3], Block::Item { depth: 0, mark, .. } if mark == "2."));
        let Block::Item { depth: 1, mark, spans } = &b[4] else { panic!("{:?}", b[4]) };
        assert_eq!(mark, "•");
        assert!(spans.iter().any(|s| s.bold && s.text == "bold"));
        let Block::Table { head, rows } = &b[5] else { panic!("{:?}", b[5]) };
        assert_eq!(head.len(), 2);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0][0][0].link, Some(Target::Param("P02-00".into())));
        assert_eq!(b[6], Block::Rule);
        assert_eq!(b.len(), 7);
    }

    #[test]
    fn loose_items_keep_one_mark() {
        let b = parse("- First\n\n  More of the first.\n\n- Second\n");
        let marks: Vec<&str> = b.iter().map(|x| if let Block::Item { mark, .. } = x { mark.as_str() } else { "?" }).collect();
        assert_eq!(marks, ["•", "", "•"]);
    }
}
