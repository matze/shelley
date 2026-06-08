use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, Stylize};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode, size};
use crossterm::{ExecutableCommand, cursor, execute, queue};

use crate::propose::{Candidate, Selection};
use crate::syntax::{self, Class, Span};

#[derive(Clone, Copy)]
enum Emphasis {
    Normal,
    Bold,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Up,
    Down,
    Accept,
    Cancel,
}

pub fn action_for(key: KeyEvent) -> Option<Action> {
    match (key.code, key.modifiers) {
        (KeyCode::Char('c'), KeyModifiers::CONTROL) => Some(Action::Cancel),
        (KeyCode::Char('j'), _) | (KeyCode::Down, _) => Some(Action::Down),
        (KeyCode::Char('k'), _) | (KeyCode::Up, _) => Some(Action::Up),
        (KeyCode::Enter, _) => Some(Action::Accept),
        (KeyCode::Esc, _) | (KeyCode::Char('q'), _) => Some(Action::Cancel),
        _ => None,
    }
}

/// Renders each candidate as two rows: the command on the first, its
/// explanation indented on the second. Nothing is truncated, so commands and
/// descriptions wider than the terminal wrap naturally onto further rows.
fn render_colored(selection: &Selection) -> Vec<String> {
    selection
        .candidates()
        .iter()
        .enumerate()
        .flat_map(|(row, candidate)| {
            let selected = row == selection.cursor();
            let marker = match selected {
                true => "❯".with(Color::Cyan).bold().to_string(),
                false => " ".to_string(),
            };
            let emphasis = match selected {
                true => Emphasis::Bold,
                false => Emphasis::Normal,
            };
            let command = highlight(&candidate.command, emphasis);
            let explanation = explanation(&candidate.explanation, selected);
            [format!("{marker} {command}"), format!("  {explanation}")]
        })
        .collect()
}

fn terminal_width() -> usize {
    size()
        .map(|(cols, _)| cols as usize)
        .ok()
        .filter(|&cols| cols > 0)
        .unwrap_or(80)
}

/// Counts the visible columns of a rendered line, skipping the ANSI escape
/// sequences emitted by the styling so that wrapping is measured against the
/// printable text only.
fn visible_width(line: &str) -> usize {
    let mut width = 0;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for escaped in chars.by_ref() {
                if escaped.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            width += 1;
        }
    }
    width
}

/// The number of physical terminal rows the lines occupy once soft wrapping at
/// `term_width` is taken into account. The selection cursor relies on this to
/// move back over exactly what was drawn.
fn physical_rows(lines: &[String], term_width: usize) -> u16 {
    lines
        .iter()
        .map(|line| visible_width(line).max(1).div_ceil(term_width) as u16)
        .sum()
}

fn highlight(command: &str, emphasis: Emphasis) -> String {
    syntax::spans(command)
        .iter()
        .map(|span| paint_span(span, emphasis))
        .collect()
}

fn paint_span(span: &Span, emphasis: Emphasis) -> String {
    let styled = match color_of(span.class) {
        Some(color) => span.text.clone().with(color),
        None => span.text.clone().stylize(),
    };
    match emphasis {
        Emphasis::Bold => styled.bold().to_string(),
        Emphasis::Normal => styled.to_string(),
    }
}

fn color_of(class: Class) -> Option<Color> {
    match class {
        Class::Command => Some(Color::Green),
        Class::Flag => Some(Color::Yellow),
        Class::Operator => Some(Color::Magenta),
        Class::Str => Some(Color::Cyan),
        Class::Var => Some(Color::Blue),
        Class::Plain | Class::Space => None,
    }
}

fn explanation(text: &str, selected: bool) -> String {
    match selected {
        true => text.to_string(),
        false => text.stylize().dim().to_string(),
    }
}

pub fn select(selection: &mut Selection) -> io::Result<Option<Candidate>> {
    let mut out = io::stderr();
    enable_raw_mode()?;
    let _ = out.execute(cursor::Hide);
    let outcome = interact(&mut out, selection);
    let _ = out.execute(cursor::Show);
    let _ = disable_raw_mode();
    outcome
}

fn interact(out: &mut impl Write, selection: &mut Selection) -> io::Result<Option<Candidate>> {
    let rows = physical_rows(&render_colored(selection), terminal_width());
    draw(out, selection)?;
    loop {
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match action_for(key) {
            Some(Action::Up) => {
                selection.up();
                redraw(out, selection, rows)?;
            }
            Some(Action::Down) => {
                selection.down();
                redraw(out, selection, rows)?;
            }
            Some(Action::Accept) => {
                erase(out, rows)?;
                return Ok(Some(selection.selected().clone()));
            }
            Some(Action::Cancel) => {
                erase(out, rows)?;
                return Ok(None);
            }
            None => {}
        }
    }
}

fn draw(out: &mut impl Write, selection: &Selection) -> io::Result<()> {
    for line in render_colored(selection) {
        write!(out, "{line}\r\n")?;
    }
    out.flush()
}

fn redraw(out: &mut impl Write, selection: &Selection, rows: u16) -> io::Result<()> {
    queue!(
        out,
        cursor::MoveUp(rows),
        cursor::MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    )?;
    draw(out, selection)
}

fn erase(out: &mut impl Write, rows: u16) -> io::Result<()> {
    execute!(
        out,
        cursor::MoveUp(rows),
        cursor::MoveToColumn(0),
        Clear(ClearType::FromCursorDown)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(selection: &Selection) -> String {
        selection
            .candidates()
            .iter()
            .enumerate()
            .flat_map(|(row, candidate)| {
                let marker = if row == selection.cursor() { '>' } else { ' ' };
                [
                    format!("{marker} {}", candidate.command),
                    format!("  {}", candidate.explanation),
                ]
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn candidates() -> Vec<Candidate> {
        vec![
            Candidate {
                command: "ls -lS".into(),
                explanation: "by size".into(),
            },
            Candidate {
                command: "du -sh *".into(),
                explanation: "disk usage".into(),
            },
        ]
    }

    #[test]
    fn maps_vi_and_arrow_keys() {
        assert_eq!(action_for(key(KeyCode::Char('j'))), Some(Action::Down));
        assert_eq!(action_for(key(KeyCode::Down)), Some(Action::Down));
        assert_eq!(action_for(key(KeyCode::Char('k'))), Some(Action::Up));
        assert_eq!(action_for(key(KeyCode::Up)), Some(Action::Up));
        assert_eq!(action_for(key(KeyCode::Enter)), Some(Action::Accept));
        assert_eq!(action_for(key(KeyCode::Char('q'))), Some(Action::Cancel));
        assert_eq!(action_for(key(KeyCode::Esc)), Some(Action::Cancel));
        assert_eq!(
            action_for(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Action::Cancel)
        );
        assert_eq!(action_for(key(KeyCode::Char('x'))), None);
    }

    #[test]
    fn visible_width_ignores_ansi_escapes() {
        let plain = "ls -lS";
        let styled = "ls -lS".green().bold().to_string();
        assert_eq!(visible_width(plain), 6);
        assert_eq!(visible_width(&styled), 6);
    }

    #[test]
    fn physical_rows_counts_wrapped_lines() {
        let lines = vec![
            "a".repeat(10),  // 1 row at width 10
            "b".repeat(11),  // wraps to 2 rows
            String::new(),   // empty line still occupies 1 row
        ];
        assert_eq!(physical_rows(&lines, 10), 4);
    }

    #[test]
    fn renders_with_marker_on_first_row() {
        let selection = Selection::new(candidates());
        insta::assert_snapshot!(render(&selection));
    }

    #[test]
    fn renders_marker_following_the_cursor() {
        let mut selection = Selection::new(candidates());
        selection.down();
        insta::assert_snapshot!(render(&selection));
    }
}
