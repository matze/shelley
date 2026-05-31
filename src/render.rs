use std::io::{self, IsTerminal, Write};

use termimad::{MadSkin, terminal_size};

const MAX_WIDTH: usize = 80;

pub fn answer(text: &str) -> io::Result<()> {
    if io::stdout().is_terminal() {
        let (cols, _) = terminal_size();
        let width = (cols as usize).min(MAX_WIDTH);
        print!("{}", MadSkin::no_style().text(text, Some(width)));
        return Ok(());
    }
    let mut out = io::stdout();
    write!(out, "{text}")?;
    if !text.ends_with('\n') {
        writeln!(out)?;
    }
    Ok(())
}
