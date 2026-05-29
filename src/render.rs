use std::io::{self, IsTerminal, Write};

use termimad::MadSkin;

pub fn answer(text: &str) -> io::Result<()> {
    if io::stdout().is_terminal() {
        MadSkin::no_style().print_text(text);
        return Ok(());
    }
    let mut out = io::stdout();
    write!(out, "{text}")?;
    if !text.ends_with('\n') {
        writeln!(out)?;
    }
    Ok(())
}
