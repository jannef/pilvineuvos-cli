# pnmail — pilvineuvos CLI

Command-line client for the throwaway inboxes at `<name>@pilvineuvos.com`
(API: `https://inbox.pilvineuvos.com/api/`). Messages are kept for ~23–24 hours.

## Install

```sh
cargo install --git https://github.com/jannef/pilvineuvos-cli
```

## API key

The API needs a key. Save it once (you'll be prompted, so it stays out of your shell history):

```sh
pnmail login                # paste the key; it is checked against the API before saving
pnmail logout               # forget it
```

## Shell completion

Tab completes commands, options and **message IDs from your current inbox**
(fish and zsh show each message's subject and age next to its ID):

```sh
# fish
echo 'COMPLETE=fish pnmail | source' > ~/.config/fish/completions/pnmail.fish
# zsh (~/.zshrc)
source <(COMPLETE=zsh pnmail)
# bash (~/.bashrc)
source <(COMPLETE=bash pnmail)
```

ID completion uses the inbox from `-i/--inbox` if you've typed it, otherwise the current inbox
(`pnmail use` / `PILVINEUVOS_INBOX`), and your saved API key.
If the API can't be reached within a second, only commands and options are completed.

## Usage

```sh
pnmail new                  # random inbox (e.g. brisk-otter-4821@pilvineuvos.com), becomes current
pnmail new --prefix signup  # signup-x7k2qp@pilvineuvos.com
pnmail use foo              # switch current inbox (foo or foo@pilvineuvos.com)
pnmail whoami               # show current address

pnmail ls [INBOX]           # list messages (age, expiry, sender, subject)
pnmail read [ID]            # newest message if no ID; --html, --headers, --raw
pnmail save [ID] -o mail.eml --attachments ./files

pnmail wait --timeout 5m    # block until a new message arrives (exit 2 on timeout)
pnmail wait --code          # ...and print just the one-time code
pnmail wait --link --from github --subject verify
pnmail watch                # stream new messages until Ctrl-C

pnmail code [ID]            # extract a one-time code
pnmail links [ID]           # extract all links
```

Add `--json` to any command for machine-readable output.

### Scripting example

```sh
EMAIL=$(pnmail new --json | jq -r .address)
./signup.sh "$EMAIL"
CODE=$(pnmail wait --code --timeout 2m)
```

`pnmail wait --since 1m` also accepts mail that arrived shortly before you started waiting.

## Configuration

| Variable | Purpose |
|---|---|
| `PILVINEUVOS_API_KEY` | API key (overrides the key saved by `pnmail login`) |
| `PILVINEUVOS_INBOX` | Inbox to use when none is given (overrides the saved current inbox) |
| `PILVINEUVOS_API` | API base URL (default `https://inbox.pilvineuvos.com`) |
| `NO_COLOR` | Disable colored output |

The current inbox and API key are stored in your config dir (`~/Library/Application Support/pilvineuvos/` on macOS,
`~/.config/pilvineuvos/` on Linux). The key file is readable only by you.

## Server side

The Cloudflare Worker scripts live in [`cloudflare/`](cloudflare/).
