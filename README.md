# shelley

A minimal oneshot shell agent — a Rust implementation of the ["comma and a
question mark"](https://www.thetypicalset.com/blog/a-comma-and-a-question-mark)
model:

- **`,` (propose)** — describe a task in plain English and get a short list of
  candidate shell commands, each with a one-line explanation. Pick one with
  `j`/`k` and Enter; it lands on your prompt for review. Shelley never runs it.
- **`?` (ask)** — ask a question and get a Markdown answer, using read-only
  tools (read a file, list a directory, fetch a URL). No writes, no shell
  execution.

## Build

```sh
cargo build --release
# binary at target/release/shelley
```

## Configuration

Set the API key for your provider:

```sh
export OPENAI_API_KEY=...     # default provider
export DEEPSEEK_API_KEY=...   # with --provider deepseek
```

Global flags (work on any subcommand):

- `--provider openai|deepseek` (default `openai`)
- `--model <name>` — override the provider's default model
- `--sandbox enabled|disabled` (default `disabled`) — run read-only file tools
  inside a [bubblewrap](https://github.com/containers/bubblewrap) sandbox
  (`bwrap` must be installed)

## Usage

```sh
# propose: suggestions printed; selected command goes to stdout
shelley propose "list the five largest files in this directory"

# ask: streamed Markdown answer using read-only tools
shelley ask "summarize README.md"
shelley --provider deepseek ask "what does src/ask.rs do?"
```

## Shell integration

The `,` and `?` prefixes are wired up by sourcing the integration script:

```sh
# zsh — in ~/.zshrc
eval "$(shelley shell-init zsh)"

# bash — in ~/.bashrc
eval "$(shelley shell-init bash)"
```

- **zsh**: press **Enter** on a line beginning with `,` or `?`. A `,` line
  replaces your prompt with the chosen command (press Enter again to run it); a
  `?` line runs the answer.
- **bash**: readline can't cleanly re-dispatch Enter, so press **Ctrl-G** to
  process the current line instead.

## Completions

```sh
shelley completions zsh  > ~/.zsh/completions/_shelley
shelley completions bash > /etc/bash_completion.d/shelley
```

(`shelley completions --help` lists supported shells.)
