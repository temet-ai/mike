# Security policy

mike sits between Claude Code and the Anthropic API and records every request and response
in a local PostgreSQL database. It reads and writes those conversations, which include file
contents and tool results, and it forwards the caller's own Anthropic credential without
storing it. It is built for one person on one machine, bound to loopback, with no
authentication on its local ports. A problem is a security problem when it lets something
other than a local process of the same user read or change that data, or makes mike send it
somewhere it was not configured to go. [`docs/SECURITY.md`](docs/SECURITY.md) describes the
trust boundary in full.

## Reporting a vulnerability

Report it privately through GitHub: on [temet-ai/mike](https://github.com/temet-ai/mike),
open the **Security** tab and choose **Report a vulnerability**. Please do not open a public
issue for it, and do not put captured conversations or credentials in a report.

Say what you found, how to reproduce it, and which version (`mike --version`) and platform
you used. We aim to reply within a week. Fixes ship in the next release rather than as
patches to older ones.

## Supported versions

Only the latest release is supported.

## What mike does with your credentials

mike holds no Anthropic credential of its own. It forwards the credential Claude Code sends
to the upstream it is configured for (`https://api.anthropic.com` by default), and does not
write it to the database or the log. The database password of a managed database is a
random value kept in an owner-only file under mike's state folder. What else is stored and
sent is listed in [`docs/WHAT_MIKE_STORES_AND_SENDS.md`](docs/WHAT_MIKE_STORES_AND_SENDS.md).
