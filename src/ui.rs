use std::io::{self, Write};

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};
use crossterm::{ExecutableCommand, cursor, execute, queue};

use crate::propose::{Candidate, Selection};

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

pub fn render(selection: &Selection) -> String {
    selection
        .candidates()
        .iter()
        .enumerate()
        .map(|(row, candidate)| {
            let marker = if row == selection.cursor() { '>' } else { ' ' };
            format!("{marker} {}  {}", candidate.command, candidate.explanation)
        })
        .collect::<Vec<_>>()
        .join("\n")
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
    for line in render(selection).split('\n') {
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
