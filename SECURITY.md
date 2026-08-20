# Security Policy

## Reporting a vulnerability

**Please do not report security vulnerabilities through public issues.**

Report privately through GitHub's **Report a vulnerability** flow: go to the
repository's **Security** tab → **Advisories** → **Report a vulnerability**. This
opens a private advisory visible only to you and the maintainer.

Include enough to reproduce: what you did, what happened, and the version
(`amenbo --version`) and platform.

## What to expect

Amenbo is maintained by one person, so responses are best-effort rather than on a
fixed schedule. You can expect an acknowledgement that the report was received, an
assessment of whether it is a vulnerability, and — if it is — a fix in a following
release with credit to you if you'd like it. If a report turns out not to be a
security issue, you'll be told why.

## Scope

Amenbo is local-first: your data lives in a single SQLite store on your machine, and
the app has no server and makes no network calls of its own beyond a version check
that reads a small static file from this repository's releases. Reports most relevant
to that model — store handling, the update check, and the release/distribution
pipeline — are especially welcome.
