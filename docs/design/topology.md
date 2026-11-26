# Topology: spec note

**In short:** topology finds which routes share which streets. It cuts the routes into pieces
where the set of routes on a street changes, so each piece of street (an edge) knows its routes
and their directions, and each route knows the pieces it runs along, in order. Ordering and
geometry use only this. The pictures below show each step on a small example.

Builds on the published work of Bast, Brosi and Storandt, Brosi's dissertation and Brosi &
Bast, with changes of its own ("How it relates to the papers", below); none of LOOM's code
(GPL-3.0) is used. Code: `crates/transit-braid-core/src/topology/`.
References: [BBS19] ([Bast, Brosi, Storandt, ACM TSAS 2019](https://arxiv.org/abs/1710.02226), §2), [B22]
([Brosi's dissertation](https://ad-research.cs.uni-freiburg.de/tmp/diss-patrick-brosi.pdf), Ch. 3), [BB24]
([Brosi and Bast, The Cartographic Journal, 2024](https://ad-publications.cs.uni-freiburg.de/Large-Scale_Generation_of_Transit_Maps_from_OpenStreetMap_Data.pdf)).

## The problem

Input: up to 70 routes in meters, in a plane. The pipeline first turns `[lng, lat]` into meters
around the network's center with plain multiplication: no `cos` or `sin` per point, as those can
differ in the last digit between machines, so every machine gets the same numbers. Output: a
**line graph**. Nodes are points; each edge has a centerline, the set of routes on it, and each
one's direction (`Forward`, `Backward`, or `Both` for a route that goes down the street and
comes back the same way: it gets one lane there, not two). Each route also gets its **walk**: its edges in order, each with its direction.
Ordering and geometry use only these.

Topology runs once, with a fixed merge distance in meters (default 5 m), so it is the same at
every zoom. It is deterministic: the same input gives the same graph, byte for byte
(C6), and native and WASM agree to the bit.

## Our method, step by step

| Step | Picture                                                   | What it does                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | Code                                        |
| ---- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| 0    | <img src="img/topology-1.svg" width="240" alt="Step 0">   | Check the input (finite points, two distinct points per route, settings in range); errors, not panics. Remove short **spikes**: out and back within 5 m, legs up to 20 m (stop pull-ins in GTFS shapes)                                                                                                                                                                                                                                                                                                                         | `Params::validate`, `despike`               |
| 1    | <img src="img/topology-2.svg" width="240" alt="Step 1">   | **Sample** each route every 2 m                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | `sample`                                    |
| 2    | <img src="img/topology-3.svg" width="240" alt="Step 2">   | Give each sample a **key**: the set of routes within the merge distance (5 m) that head the same way or the opposite way, within 30° (so a street that only crosses at a junction doesn't count, while both directions of one street do), itself included, plus whether its own route passes there again (farther than 4 × merge along it). To find what is near a sample quickly, the routes' segments are filed in a **grid index**: the map cut into 5 m squares, so only the sample's square and its neighbors are searched | `key`, `Grid`                               |
| 3    | <img src="img/topology-4.svg" width="240" alt="Step 3">   | **Smooth islands:** a stretch shorter than 20 m between two stretches with the same key takes that key                                                                                                                                                                                                                                                                                                                                                                                                                          | `smooth_islands`                            |
| 4    | <img src="img/topology-5.svg" width="240" alt="Step 4">   | Consecutive samples with one key form a **run**. Runs with the same set that lie on top of each other become one **edge**; its centerline is its longest run                                                                                                                                                                                                                                                                                                                                                                    | `build_with_traversals`, `overlaps`         |
| 5    | <img src="img/topology-6.svg" width="240" alt="Step 5">   | Each run's **direction** along its edge (forward, backward, or both for out and back), read by where its first and last samples fall along the centerline (the middle sample only for a run about one sample long)                                                                                                                                                                                                                                                                                                              | `runs_along`                                |
| 6    | <img src="img/topology-7.svg" width="240" alt="Step 6">   | **Split** an edge where one of its routes joins or leaves it while the others carry on, and where a loop line starts or ends on an edge it runs again. A run of a few meters that reaches no piece's middle but ends by a cut or an edge end is a **point** there: it covers no piece and adds no direction                                                                                                                                                                                                                     | `split_at_run_ends`                         |
| 7    | <img src="img/topology-8.svg" width="240" alt="Step 7">   | **Nodes:** edge ends closer than the merge distance join, and so do the two ends where one route's run ends and its next begins (continuity). A node is the average of its ends                                                                                                                                                                                                                                                                                                                                                 | `build_with_traversals`                     |
| 8–9  | <img src="img/topology-9.svg" width="240" alt="Step 8–9"> | **Edges:** centerline simplified (0.5 m), ends set to their nodes. An edge whose two ends became one node is dropped up to 4 × merge long (a set-change artifact), split at its middle beyond that (a circular route). **Walks:** each route's runs, in order, become its steps through the edges                                                                                                                                                                                                                               | `edge`, `simplify`, `build_with_traversals` |

A plain crossing of two streets is not a node: each line keeps its own edge through it.

## How it relates to the papers

**Method A** [BBS19 §2] is the family ours belongs to: find shared segments between pairs of
edges (every point of one part within d̂ of the other, swept in 10 m steps), collapse each into
one edge with an averaged path and the union of lines, put degree-3 nodes at its ends, merge new
nodes closer than d̂ to old ones, and repeat until no shared segment is left, merging in random
order. The authors later dropped it for its artifacts [B22 §3.1], for **method B**, snapping each
sample onto the nearest node [B22 §3.2, BB24]. We measured method B on real networks: junctions
come out clean only at 30 m and more, where lines drift 26–65 m from their streets. It was
dropped; ours was kept.

Where ours differs from method A, and why:

| Method A [BBS19 §2]                            | Ours                                               | Why                                                                                                                        |
| ---------------------------------------------- | -------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| d̂ from line widths: w(\|L(e1)\| + \|L(e2)\|)/2 | A fixed 5 m in meters                              | Topology must not depend on zoom or widths                                                                                 |
| Pairs of edges, repeated until nothing changes | Every sample against every route at once, one pass | One pass is deterministic and fast enough (a 50-route city's first draw takes under a second in WASM, an edit about 0.1 s) |
| Random merge order                             | Fixed order                                        | The same graph on every machine (C6)                                                                                       |
| Averaged path for a shared segment             | The longest run's own path                         | Averaging rounded bends inward and moved edge ends at junctions, failing C1 on a real city                                 |

Ours also adds two things for Braid: a heading test (within 30°, either way), so crossing
streets don't merge while both directions of a street do (method B has no angle test, [B22]);
and out-and-back handling, so a route on a street twice keeps one slot (direction `Both`,
walked twice; the key's "passes there again" in step 2).

Turn restrictions [B22 §3.3] are not modeled: a walk is a route's own path, so it never takes
a turn the route doesn't.

## Results

C1 is measured at each input point and every 5 m along each route. On the committed fixtures
(`crates/transit-braid-testkit`, `criteria.rs` and the topology test):

| Fixture              | Nodes / edges | C1 max / p95  | C2  | C3  | C4 (expected graph) | C5  |
| -------------------- | ------------- | ------------- | --- | --- | ------------------- | --- |
| 6 hand-made fixtures | match         | ≤ 2.9 / 2.5 m | 0   | 0   | match               | 0   |
| y-split              | 5 / 4         | 3.6 / 3.05 m  | 0   | 0   | match               | 0   |
| out-and-back         | 3 / 2         | 4.9 / 4.9 m   | 0   | 0   | match               | 0   |

On real city networks of 50 lines (not in this repo) C2 and C5 pass, C1's 95th percentile stays
within 2.1 m, and the failures are the known limits below.

**Known limits:**

- **Flicker (C3):** short edges between two set changes (turnarounds, junctions close
  together); cosmetic, handled with the look.
- **A corner of two lanes:** two lanes' ends averaged into one node can sit about 6 m off.
- **C1 next to a junction:** a node that sits off a route, as the average of nearby edge ends
  (up to about 8.5 m on a real city; y-split's p95 is 3.05 m against 3 m).
- **out-and-back's U-turn tip:** 4.9 m from the centerline; no centerline is near a U-turn's tip.
- **Wide streets:** the two sides of a wide street can stay two bundles; cosmetic.
- **Bad input stays bad:** an input route that runs out and back for a long way stays so.

Tried and not kept: bending each edge's last 20 m onto its node (C1 better, self-crossings
doubled), and placing nodes where their streets meet (C1's p95 halved, the failing maximums
unchanged, more flicker).

## Edits: incremental rebuild

A sample's key (step 2) is the only part of the build that looks at other routes, and it is
about 80% of the time on a city. `build_incremental` keeps each route's samples (steps 0–1)
and every sample's key from the last call. A new call compares the routes with the last ones
(an unchanged start and end, and between them a run of routes added, removed or changed) and
computes a key again only where it can differ:

- every sample of an added or changed route;
- a kept route's sample whose key named a removed or changed route;
- a kept route's sample within reach of an added or changed one (its grid cells, one cell of
  5 m around them: exactly where the key could see it).

Other keys are kept (their route numbers shifted when routes were added or removed before
them). Steps 3–9 then run as in a full build, so the graph and walks are exactly a full build's
(a property test checks it over random street-grid inputs and two edits in a row). Settings
changed, or more than half of the routes new: a full build. The pipeline's cache uses it,
so an edit in `sideBySide` computes 14–25% of the keys on a real city network.

## Tests (what proves it)

In `topology/`'s `*_tests.rs` files, all on plain routes (no fixtures, no mocks):

- **Shapes:** a Y split, two crossing lines, a shared stretch that splits into a bundle and
  branches, opposite directions on one edge, out-and-back (one slot, walked twice), a spike on a
  shared stretch, a circular route, a route that loops off and rejoins, lines farther apart than the merge
  distance.
- **Corners:** a V-shaped run whose middle sample sits on the corner (both ways), runs about one
  sample long, a shallow crossing, a zigzag after a shared stretch (both fail without the fix).
- **Bad input:** empty and single inputs, NaN and infinite points, routes without two distinct
  points, every bad setting; errors, never panics.
- **Properties (proptest):** any input gives a well-formed, deterministic graph; walks agree with
  the edges; every node is within 2 × merge of the input; `despike` keeps both ends and the order.

The testkit checks the graph criteria on every committed fixture (`criteria.rs`), and the
pipeline's property tests check the drawn lines against their walks and inputs.
