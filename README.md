# pn — pilvineuvos CLI

Command-line client for the throwaway inboxes at `<name>@pilvineuvos.com`
(API: `https://inbox.pilvineuvos.com/api/`). Messages are kept for ~23–24 hours.

## Install

```sh
cargo install --git https://github.com/jannef/pilvineuvos-cli
```

## Usage

```sh
pn new                      # random inbox (e.g. brisk-otter-4821@pilvineuvos.com), becomes current
pn new --prefix signup      # signup-x7k2qp@pilvineuvos.com
pn use foo                  # switch current inbox (foo or foo@pilvineuvos.com)
pn whoami                   # show current address

pn ls [INBOX]               # list messages (age, expiry, sender, subject)
pn read [ID]                # newest message if no ID; --html, --headers, --raw
pn save [ID] -o mail.eml --attachments ./files

pn wait --timeout 5m        # block until a new message arrives (exit 2 on timeout)
pn wait --code              # ...and print just the one-time code
pn wait --link --from github --subject verify
pn watch                    # stream new messages until Ctrl-C

pn code [ID]                # extract a one-time code
pn links [ID]               # extract all links
```

Add `--json` to any command for machine-readable output.

### Scripting example

```sh
EMAIL=$(pn new --json | jq -r .address)
./signup.sh "$EMAIL"
CODE=$(pn wait --code --timeout 2m)
```

`pn wait --since 1m` also accepts mail that arrived shortly before you started waiting.

## Configuration

| Variable | Purpose |
|---|---|
| `PILVINEUVOS_INBOX` | Inbox to use when none is given (overrides the saved current inbox) |
| `PILVINEUVOS_API` | API base URL (default `https://inbox.pilvineuvos.com`) |
| `NO_COLOR` | Disable colored output |

The current inbox is stored in your config dir (`~/Library/Application Support/pilvineuvos/current` on macOS,
`~/.config/pilvineuvos/current` on Linux).
