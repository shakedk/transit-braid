# Security policy

## Reporting a vulnerability

Please report security problems privately, through GitHub's form:
[Report a vulnerability](https://github.com/shakedk/transit-braid/security/advisories/new) (the
repository's **Security** tab, then **Report a vulnerability**). Only the maintainer sees the
report. Please don't open a public issue, pull request or discussion for it.

A useful report says:

- which package and version (`transit-braid`, `transit-braid-deck`; `npm ls transit-braid`)
- what an attacker can do, and what they need to control (the routes, the stations, the options)
- the smallest input or steps that show it, if you have them

## What happens next

transit-braid has one maintainer, so these are aims, not promises:

- You get an answer within 7 days saying whether we can reproduce it.
- We agree with you on a date to make it public, normally within 90 days, sooner once a fix
  is out.
- The fix ships in a new release, with a GitHub security advisory (and a CVE when it applies)
  that credits you, unless you'd rather not be named.

## Supported versions

transit-braid is before 1.0. Only the newest minor version gets security fixes.

| Version | Security fixes       |
| ------- | -------------------- |
| 0.1.x   | Yes                  |
| < 0.1   | No (never published) |

## In scope

The published npm packages `transit-braid` and `transit-braid-deck`: their TypeScript code, the
WebAssembly engine inside them and the Web Worker that runs it. For example:

- input (GeoJSON routes, stations or options) that runs code, reads or writes outside the
  engine's memory, or changes objects it shouldn't (prototype pollution)
- input that gets past the documented limits and work budget (DEVELOPMENT.md, "Input limits
  and work budget"), so a call runs for minutes or exhausts memory instead of being refused
- a crash that leaves the library unusable instead of failing only that call
- the release process: how the packages are built and published from this repository

## Out of scope

- Slow calls on input inside the documented limits: the limits are there to keep every call
  bounded, so report the limit you'd want, as a regular issue.
- Problems in a dependency, the browser or the map library you use with transit-braid: please
  report them to that project (and tell us if transit-braid needs to change too).
- The demo pages and their map tiles, which are examples, not part of the packages.
- The Rust crates in this repository, which are not published.
