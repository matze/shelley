use std::io::{self, IsTerminal, Write};

use termimad::crossterm::style::{Attribute, Color};
use termimad::{CompoundStyle, MadSkin, StyledChar, terminal_size};

const MAX_WIDTH: usize = 80;

pub fn answer(text: &str) -> io::Result<()> {
    if io::stdout().is_terminal() {
        let (cols, _) = terminal_size();
        let width = (cols as usize).min(MAX_WIDTH);
        print!("{}", skin().text(text, Some(width)));
        return Ok(());
    }
    let mut out = io::stdout();
    write!(out, "{text}")?;
    if !text.ends_with('\n') {
        writeln!(out)?;
    }
    Ok(())
}

fn skin() -> MadSkin {
    let mut skin = MadSkin::no_style();
    skin.bold.add_attr(Attribute::Bold);
    skin.italic.add_attr(Attribute::Italic);
    skin.inline_code.set_fg(Color::Green);
    skin.code_block.compound_style.set_fg(Color::Green);
    for header in &mut skin.headers {
        header.compound_style.set_fg(Color::Cyan);
        header.compound_style.add_attr(Attribute::Bold);
    }
    skin.headers[0]
        .compound_style
        .add_attr(Attribute::Underlined);
    skin.bullet = StyledChar::new(CompoundStyle::with_fg(Color::Cyan), '•');
    skin.quote_mark = StyledChar::new(
        CompoundStyle::new(Some(Color::Cyan), None, Attribute::Bold.into()),
        '▐',
    );
    skin
}
