#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Class {
    Command,
    Flag,
    Operator,
    Str,
    Var,
    Plain,
    Space,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Span {
    pub class: Class,
    pub text: String,
}

const OPERATORS: &[char] = &['|', '&', ';', '<', '>'];

pub fn spans(command: &str) -> Vec<Span> {
    let chars: Vec<char> = command.chars().collect();
    let mut spans = Vec::new();
    let mut at = 0;
    let mut expect_command = true;

    while at < chars.len() {
        let start = at;
        let class = match chars[at] {
            c if c.is_whitespace() => {
                at = advance_while(&chars, at, |c| c.is_whitespace());
                Class::Space
            }
            c if OPERATORS.contains(&c) => {
                at = advance_while(&chars, at, |c| OPERATORS.contains(&c));
                expect_command = true;
                Class::Operator
            }
            '\'' | '"' => {
                at = scan_quoted(&chars, at);
                expect_command = false;
                Class::Str
            }
            '$' => {
                at = scan_variable(&chars, at);
                expect_command = false;
                Class::Var
            }
            _ => {
                at = advance_while(&chars, at, is_bare);
                let class = classify_bare(&chars[start..at], expect_command);
                expect_command = false;
                class
            }
        };
        spans.push(Span {
            class,
            text: chars[start..at].iter().collect(),
        });
    }

    spans
}

fn is_bare(c: char) -> bool {
    !c.is_whitespace() && !OPERATORS.contains(&c) && c != '\'' && c != '"' && c != '$'
}

fn classify_bare(word: &[char], expect_command: bool) -> Class {
    match (expect_command, word.first()) {
        (true, _) => Class::Command,
        (false, Some('-')) => Class::Flag,
        _ => Class::Plain,
    }
}

fn scan_quoted(chars: &[char], start: usize) -> usize {
    let quote = chars[start];
    let mut at = start + 1;
    while at < chars.len() {
        if chars[at] == quote {
            return at + 1;
        }
        at += 1;
    }
    at
}

fn scan_variable(chars: &[char], start: usize) -> usize {
    match chars.get(start + 1) {
        Some('{') => {
            let end = advance_while(chars, start + 2, |c| c != '}');
            if end < chars.len() { end + 1 } else { end }
        }
        _ => advance_while(chars, start + 1, |c| c.is_alphanumeric() || c == '_'),
    }
}

fn advance_while(chars: &[char], from: usize, keep: impl Fn(char) -> bool) -> usize {
    let mut at = from;
    while at < chars.len() && keep(chars[at]) {
        at += 1;
    }
    at
}

#[cfg(test)]
mod tests {
    use super::*;

    fn classes(command: &str) -> Vec<(Class, String)> {
        spans(command)
            .into_iter()
            .map(|span| (span.class, span.text))
            .collect()
    }

    #[test]
    fn round_trips_the_original_text() {
        for command in ["ls -lS", "du -sh * | sort -h", "echo \"a b\"  $HOME"] {
            let joined: String = spans(command).into_iter().map(|span| span.text).collect();
            assert_eq!(joined, command);
        }
    }

    #[test]
    fn first_word_is_the_command_then_flags_and_plain() {
        assert_eq!(
            classes("ls -lS path"),
            vec![
                (Class::Command, "ls".into()),
                (Class::Space, " ".into()),
                (Class::Flag, "-lS".into()),
                (Class::Space, " ".into()),
                (Class::Plain, "path".into()),
            ]
        );
    }

    #[test]
    fn operator_starts_a_new_command() {
        assert_eq!(
            classes("du -sh | sort"),
            vec![
                (Class::Command, "du".into()),
                (Class::Space, " ".into()),
                (Class::Flag, "-sh".into()),
                (Class::Space, " ".into()),
                (Class::Operator, "|".into()),
                (Class::Space, " ".into()),
                (Class::Command, "sort".into()),
            ]
        );
    }

    #[test]
    fn quotes_and_variables_are_distinct() {
        assert_eq!(
            classes("grep \"a b\" $HOME"),
            vec![
                (Class::Command, "grep".into()),
                (Class::Space, " ".into()),
                (Class::Str, "\"a b\"".into()),
                (Class::Space, " ".into()),
                (Class::Var, "$HOME".into()),
            ]
        );
    }

    #[test]
    fn braced_variable_is_captured_whole() {
        assert_eq!(
            classes("echo ${PATH}"),
            vec![
                (Class::Command, "echo".into()),
                (Class::Space, " ".into()),
                (Class::Var, "${PATH}".into()),
            ]
        );
    }
}
