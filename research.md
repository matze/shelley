# Shelley oneshot agent

## Purpose

Shelley is a custom Rust-based CLI agent uses for the following shell tasks:

1. When asked to describe something, it should find the top five ways to do
   something on the command line, e.g. asking "list the five largest files in
   the current directory" could return "ls -lS". It should not use any tool
   calls and merely return these options it finds in its model weights. The user
   can then select and execute the command or have it explained.

2. When asked to do something, it should be able to read something either from
   the local filesystem or the web. For example "summarize README.md" should
   read the file and output and render Markdown formatted summary. This allows
   the agent to make tool calls but only those that are read-only, especially no
   generic bash tool calls.

## Technical constraints

- Use async Rust with structured concurrency primitives (e.g. futures,
  futures-concurrency) and use tokio only to access the network
- Target Linux
- Target the OpenAI API and DeepSeek v4 models
- Use strides for async spinner integration
- Offer ways to execute tool calls in a sandbox (e.g. bubble wrap)
- Use clap for CLI and completion generation
- The terminal UI should be minimal, a single spinner and/or progress bar + the
  final selection box only. No cute output at all.
- The terminal UX should be simple and Vi-like, i.e. j/k to select the command

## Plan

For each step, ask me first what your findings are and how to proceed. Do not implement anything straight away.

1. Research options to access OpenAI APIs with async Rust.

2. Present the general tool call loop and how to prevent typical failure modes
   like spending too many tokens or getting stuck in a dead end.

3. Present the general architecture that should eschew of using async tasks as
   much as possible. Ideally everything is a tree of composed futures rooted in
   `async fn main`

4. Present how you would write tests.

## Implementation

The implementation should be done in individually reviewable commits using jj
starting from basic project scaffolding, tests and stubs and then filling out
wholes. Each commit is to be made or reviewed by me.
