# Contributing

**Issues are welcome. For pull requests, please open an issue to discuss first.**

amenbo is maintained by one person. Bug reports, questions, and feature ideas are
genuinely wanted — open an issue. But please don't send a pull request out of the
blue: start with an issue so we can agree on the shape of the change before you
spend time on it. An unsolicited PR may be declined even if the code is good, simply
because it doesn't fit where the project is going. Discussing first saves that.

## Reporting a bug or asking for a feature

Open an issue. There are templates for a bug report and a feature request — filling
one in gives the maintainer what they need to act without a round-trip.

## Code of conduct

Taking part here — an issue, a pull request, a discussion — means holding to the
[Code of Conduct](CODE_OF_CONDUCT.md).

## Security

Don't file a security problem as a public issue. See [SECURITY.md](SECURITY.md) for
how to report a vulnerability privately.

## Building

Build, test, and toolchain steps live in the [README](README.md) — see the
**Toolchain** and **Build and run** sections. The one local gate that mirrors CI is
`make test`; run it before proposing a change.

Comments are audited too, against the rules declared in `esorp.yaml`: where a comment
may sit, what shape it takes, and that it is written in English. Every comment in the
tree is judged, and the tree stands at zero, so a red gate names something your change
put there. To see the same verdict before you push, install
[esorp](https://github.com/ShiroDoromoto/esorp) and run `make hooks` — without it,
committing works exactly as before.

The files that carry no code are held to the same vocabulary: the prose of every
tracked `.md`, and the values of every manifest and config file (a comment scanner
cannot see a value, so `description = "…"` answered to no one). It reads a fenced code
block as prose too, so a code span — not a fence — is where an identifier of the form
the docs describe belongs.

Source is exempt, deliberately. Its strings are half of a localized product — the GUI
dictionaries and the i18n phrasebook are meant to carry every language they cover — so
English is asked of the comments there, not the literals.

## License

By contributing, you agree that your contributions are licensed under the
[Apache License 2.0](LICENSE), the same license that covers this project.
