use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, Stylize};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
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

fn layout(selection: &Selection, line: impl Fn(&Candidate, bool, usize) -> String) -> String {
    let width = selection
        .candidates()
        .iter()
        .map(|candidate| candidate.command.chars().count())
        .max()
        .unwrap_or(0);
    selection
        .candidates()
        .iter()
        .enumerate()
        .map(|(row, candidate)| line(candidate, row == selection.cursor(), width))
        .collect::<Vec<_>>()
        .join("\n")
}

fn render_colored(selection: &Selection) -> String {
    layout(selection, |candidate, selected, width| {
        let gap = " ".repeat(width - candidate.command.chars().count());
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
        format!("{marker} {command}{gap}  {explanation}")
    })
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
    let rows = selection.candidates().len() as u16;
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
    for line in render_colored(selection).split('\n') {
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
        layout(selection, |candidate, selected, width| {
            let gap = " ".repeat(width - candidate.command.chars().count());
            let marker = if selected { '>' } else { ' ' };
            format!(
                "{marker} {}{gap}  {}",
                candidate.command, candidate.explanation
            )
        })
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
