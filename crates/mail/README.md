<div align="center">

# pku-mail

*A command-line client for the PKU student mailbox (@stu.pku.edu.cn).*

[![crates.io](https://img.shields.io/crates/v/pku-mail?style=flat-square&logo=rust)](https://crates.io/crates/pku-mail)
[![License](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](https://github.com/pkuinfo/pkucli/blob/main/LICENSE)

</div>

Read, search and send mail in your PKU student mailbox from the terminal. The student mailbox runs on NetEase Enterprise Mail; staff mail (@pku.edu.cn) is a different system and is not supported.

## Install

```bash
cargo install pku-mail
```

Or install the [complete PKU toolkit](https://crates.io/crates/pku-cli):

```bash
cargo install pku-cli
```

## Setup

The mailbox uses your student ID and IAAA password. Store them once (encrypted in your OS keychain), then log in:

```bash
cargo install pku-auth
info-auth store
mail login -p
```

A new machine needs one SMS code. In a terminal you are prompted for it; without a terminal (scripts, AI agents) the code is sent and the command exits, then `mail login -p --code 123456` finishes the login. The machine is then trusted and later logins (including automatic re-login when the session expires after about a day) need no SMS.

## Usage

```bash
mail folders                              # Folders with unread counts
mail ls                                   # Inbox, newest first
mail ls --unread                          # Unread only
mail ls -f sent --page 2                  # Sent mail, page 2
mail read <mid>                           # Read a message (stays unread)
mail read <mid> --mark-read               # ... and mark it as read
mail search 奖学金                         # Full-text search
mail download <mid> -o ./out              # All attachments
mail download <mid> 2                     # One attachment by part id
mail contacts 张                          # Personal contacts

mail send --to a@pku.edu.cn -s "Hello" --body "Hi!"          # Preview only
mail send --to a@pku.edu.cn -s "Hello" --body "Hi!" --yes    # Send
mail reply <mid> --body "Thanks!" --yes                      # Reply (quotes the original)
```

Every command accepts `--json`. Sending and replying only print a preview unless `--yes` is given.

## How It Works

The mail server checks the password itself (it does not go through IAAA): the client fetches a one-time RSA key, encrypts `password#rand` with PKCS#1 v1.5, and posts it to NetEase's domain login. Optional SMS verification trusts the device. Mail operations use the web client's `/js6/s` JSON RPC. See [`docs/stu-mail-api.md`](https://github.com/pkuinfo/pkucli/blob/main/docs/stu-mail-api.md).

## Links

- **Repository**: [github.com/pkuinfo/pkucli](https://github.com/pkuinfo/pkucli)
- **Full documentation**: See the main [README](https://github.com/pkuinfo/pkucli#readme)
