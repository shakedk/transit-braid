# Fixtures

Test networks: small hand-made ("synthetic") cases, one per known hard situation, a few bus
patterns and a 70-route stress grid, all made for this project (MIT). Every bug we find becomes
a fixture.

## Format

A fixture is a GeoJSON `FeatureCollection` of `LineString`s, one per transit line, plus a
`braid` object with metadata:

```json
{
  "type": "FeatureCollection",
  "braid": {
    "name": "y-split",
    "description": "Three lines share a trunk, then split three ways.",
    "units": "meters",
    "license": "MIT",
    "attribution": "transit-braid authors (synthetic)",
    "expect": "One shared trunk with three lanes; after the split each line runs alone."
  },
  "features": [
    {
      "type": "Feature",
      "properties": { "line": "A", "color": "#e4002b" },
      "geometry": {
        "type": "LineString",
        "coordinates": [
          [0, 0],
          [500, 0]
        ]
      }
    }
  ]
}
```

| Field               | Meaning                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `braid.name`        | Short id, same as the file name without `.geojson`                                                                                                                                                                                                                                                                                                                                                                                                               |
| `braid.description` | What situation this fixture tests                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `braid.units`       | `"meters"` (synthetic, local x/y) or `"degrees"` (real data, `[lng, lat]`)                                                                                                                                                                                                                                                                                                                                                                                       |
| `braid.license`     | License of the data (synthetic: `MIT`, like the repo)                                                                                                                                                                                                                                                                                                                                                                                                            |
| `braid.attribution` | Who made the data; required by licenses like ODbL and CC-BY                                                                                                                                                                                                                                                                                                                                                                                                      |
| `braid.expect`      | What good output looks like, in plain words, for reviewing pictures                                                                                                                                                                                                                                                                                                                                                                                              |
| `properties.line`   | Line id, unique within the fixture                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `properties.color`  | Line color as `#rrggbb`                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `properties.sign`   | Optional: the route sign riders see (e.g. `130`); several lines may share it                                                                                                                                                                                                                                                                                                                                                                                     |
| `geometry`          | `LineString` with at least two points                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `braid.stations`    | Optional stops: `[{ "id": "13679", "code": "25823", "name": "Main St", "at": [190, 8], "routes": ["130-1"] }, ...]`; `id` (the feed's stop ID; several joined by `, ` where the importer merged stops at one spot), `code` (the feed's stop code, the number riders see at the stop; joined the same way) `name` optional, never empty; `routes` optional, the ids of this fixture's lines that stop there (the importer writes it); `at` in the fixture's units |

Synthetic fixtures use meters so the numbers are easy to read. Lines that share a street are
drawn a few meters apart on purpose ("jitter"), like real data where two routes along the same
street never have identical coordinates.

The loader and validator live in `crates/transit-braid-testkit`; `cargo test -p
transit-braid-testkit` checks every fixture in this folder.

## Synthetic fixtures

| Fixture              | Tests                                                  |
| -------------------- | ------------------------------------------------------ |
| `parallel-corridor`  | Three lines on one route with a bend; the basic bundle |
| `y-split`            | A shared trunk splitting three ways                    |
| `x-crossing`         | Two corridors crossing without sharing track           |
| `dog-bone`           | Two lines that meet, share a middle stretch, and part  |
| `opposite-direction` | Same track drawn in opposite directions                |
| `hairpin`            | Two lines through a tight hairpin bend                 |

## Stress fixtures

In `stress/`: the most one call takes, for timing and the CI gate. No expected graph (too big to build by hand).

| Fixture       | Tests                                                                                     |
| ------------- | ----------------------------------------------------------------------------------------- |
| `stress-grid` | 70 routes on a 12 × 12 street grid of 200 m blocks, made by `node scripts/stress-grid.ts` |

## Bus fixtures

Hand-made cases from bus networks, in `bus/`, each with an expected graph except `loop-twice`
(an expected graph lists each street once per line, so it can't say a walk repeats one; its test
checks the topology's own walk).

| Fixture        | Tests                                                                                                                                                          |
| -------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `out-and-back` | A route that runs a street out and back takes one slot there (`"dir": "both"`)                                                                                 |
| `one-way-pair` | A one-way street pair 20 m apart stays two separate edges; the two-way ends are shared                                                                         |
| `stops`        | Stops on a shared street: a same-name pair across it shares one bar; others keep their own; a far one gets none                                                |
| `loop-line`    | A circular route that starts and ends on a street it shares: the street is cut there; the loop closes on itself                                                |
| `loop-twice`   | A route drives around a block and then along its first side again, the same way (like a real route seen in a city feed): one line, C2 counts that street twice |

## Golden digests

- `golden.json`: the golden digests of every committed fixture: a 64-bit fingerprint of the
  pipeline's lines and bars at each zoom, with lane caps and through an edit sequence. The tests
  fail on any change; see `DEVELOPMENT.md` ("Golden harness") for updating it on purpose.

## Expected line graphs

`<name>.graph.json` next to a fixture is the hand-built **line graph** that fixture should turn
into: **nodes** where the set of routes changes or a route ends, and
**edges** between them, each listing the routes on it and their direction. The testkit's
topology check compares the graph the core builds with these (C4).

```json
{
  "fixture": "opposite-direction",
  "nodes": [
    { "id": "west", "at": [0, 1] },
    { "id": "east", "at": [800, 101] }
  ],
  "edges": [
    {
      "id": "corridor",
      "from": "west",
      "to": "east",
      "via": [
        [300, 51],
        [600, 51]
      ],
      "lines": [
        { "line": "A", "dir": "forward" },
        { "line": "B", "dir": "backward" }
      ]
    }
  ]
}
```

| Field           | Meaning                                                                        |
| --------------- | ------------------------------------------------------------------------------ |
| `fixture`       | The fixture's `braid.name`                                                     |
| `nodes[].id`    | Unique, non-empty                                                              |
| `nodes[].at`    | `[x, y]` in the fixture's units                                                |
| `edges[].id`    | Unique, non-empty                                                              |
| `edges[].from`  | Node id where the edge starts; `to` is where it ends (must differ)             |
| `edges[].via`   | Optional bends of the centerline between the two nodes                         |
| `edges[].lines` | The routes on the edge, at least one, each once (one slot per route)           |
| `.dir`          | `forward` (runs `from` → `to`), `backward`, or `both` (out and back, one slot) |

Every route in the fixture must be on at least one edge. Unknown fields are rejected.
Plain crossings are **not** nodes: two bundles that only cross (no shared line, no shared stop)
keep one edge each (see `x-crossing`).
