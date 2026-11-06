# Contributing to mike

Thank you for helping. The most useful contributions right now are bug reports from real
use, and small fixes. For anything larger, open an issue first and say what you have in
mind, so we can agree on the shape before you spend time on it.

## Licence

mike is MIT licensed. By sending a pull request you agree that your contribution is
licensed under the MIT licence too. There is no contributor licence agreement to sign.

## Building and testing

From the repository root. [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md) has the toolchain
versions and the databases the tests use.

```bash
make build
make test                      # no database needed
make test-p p=<crate>          # the crate you changed
make lint
```

Frontend stories and tests run through `scripts/sb-test`, never `pnpm test`.

## Rules every change keeps

- Test data uses made-up names, never a real organisation, person, project or ticket.
  New test data takes its names from Terry Pratchett's Discworld (`Sam Vimes`,
  `ankh-morpork/watch`, `WATCH-12`). Hosts are `example.com` or `.invalid`.
- Real captures (requests, responses, transcripts) never enter the repository; write a
  synthetic fixture instead.
- A change to what mike stores or sends changes
  [`docs/WHAT_MIKE_STORES_AND_SENDS.md`](docs/WHAT_MIKE_STORES_AND_SENDS.md) in the same
  commit.

## Pull requests

Keep a pull request to one change, with tests for it. Say in the description what it does
for someone using mike and how you checked it.
