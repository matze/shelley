use clap::ValueEnum;

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum InitShell {
    Bash,
    Zsh,
}

pub fn integration(shell: InitShell) -> &'static str {
    match shell {
        InitShell::Zsh => ZSH,
        InitShell::Bash => BASH,
    }
}

const ZSH: &str = r#"# shelley zsh integration. Add to ~/.zshrc:
#   eval "$(shelley shell-init zsh)"
#
# Press Enter on a line that starts with ',' to propose a command onto the
# prompt (review it, then press Enter again to run it), or '?' to answer a
# question with the read-only agent.
_shelley_accept_line() {
  emulate -L zsh
  if [[ $BUFFER == ,* ]]; then
    local query=${BUFFER#,}
    local suggestion
    if suggestion=$(shelley propose -- "$query" </dev/tty); then
      BUFFER=$suggestion
      CURSOR=$#BUFFER
    fi
    zle reset-prompt
    return
  fi
  if [[ $BUFFER == '?'* ]]; then
    local query=${BUFFER#\?}
    BUFFER="shelley ask -- ${query}"
  fi
  zle .accept-line
}
zle -N accept-line _shelley_accept_line
"#;

const BASH: &str = r#"# shelley bash integration. Add to ~/.bashrc:
#   eval "$(shelley shell-init bash)"
#
# Press Ctrl-G to process the current line: a line starting with ',' proposes a
# command onto the prompt (review it, then press Enter to run it); a line
# starting with '?' runs an answer with the read-only agent.
_shelley_accept_line() {
  if [[ $READLINE_LINE == ,* ]]; then
    local suggestion
    if suggestion=$(shelley propose -- "${READLINE_LINE#,}" </dev/tty); then
      READLINE_LINE=$suggestion
      READLINE_POINT=${#READLINE_LINE}
    fi
  elif [[ $READLINE_LINE == '?'* ]]; then
    shelley ask -- "${READLINE_LINE#\?}" </dev/tty
    READLINE_LINE=
    READLINE_POINT=0
  fi
}
bind -x '"\C-g": _shelley_accept_line'
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_overrides_accept_line_and_routes_both_prefixes() {
        let script = integration(InitShell::Zsh);
        assert!(script.contains("zle -N accept-line _shelley_accept_line"));
        assert!(script.contains("BUFFER == ,*"));
        assert!(script.contains("shelley propose -- \"$query\""));
        assert!(script.contains("shelley ask -- ${query}"));
    }

    #[test]
    fn bash_uses_readline_line_and_routes_both_prefixes() {
        let script = integration(InitShell::Bash);
        assert!(script.contains("READLINE_LINE == ,*"));
        assert!(script.contains("shelley propose --"));
        assert!(script.contains("shelley ask --"));
        assert!(script.contains("bind -x"));
    }
}
