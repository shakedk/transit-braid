# Line ordering: design note

**In short:** where several routes share a street, ordering decides which lane each one takes,
so that routes cross each other as little as possible and routes that travel together stay
neighbors. It is decided once per street piece and is the same at every zoom. The pictures show
how a crossing is found, what an order costs, how the search works, and what an edit changes.

Builds on the published work of Bast, Brosi and Storandt, with changes of its own ("How it
relates to the papers", below); none of LOOM's code is used. Code:
`crates/transit-braid-core/src/ordering/`. References: [BBS18] ([Bast, Brosi, Storandt, ACM
SIGSPATIAL 2018](https://ad-publications.informatik.uni-freiburg.de/SIGSPATIAL_transitmaps_2018.pdf)), [BBS19] ([the same authors, ACM TSAS 2019](https://arxiv.org/abs/1710.02226)), [B22]
([Brosi's dissertation](https://ad-research.cs.uni-freiburg.de/tmp/diss-patrick-brosi.pdf)).

## The problem

The topology gives a line graph: edges carrying sets of lines, and each line's walk
through it. Ordering picks, for every edge, which side each of its lines takes. It depends only
on the graph, so it is the same at every zoom. The papers frame this as line-ordering
optimization: minimize crossings, weighted by kind, plus separations of lines that run together.

## Our model

- **An order** lists each edge's lines from right to left, looking from its `from` node to its
  `to` node (slot 0 is the rightmost), the same frame the geometry uses.
- **At a node,** picture a small circle around it. Going counterclockwise, the edges come in the
  order they leave the node, and each edge's lines from its right to its left (looking out of the
  node). A line passing through the node, from one edge to the next in its walk, is a chord
  between its two places on the circle. **Two lines cross at the node exactly when their chords
  interleave.** This one test covers every case (same side, opposite sides, any number of edges).
- **Kinds,** counted per node and per pair of lines that share at least one of their two edges
  there (a pair sharing none crosses or not whatever the order is):
  - same-segment crossing: both come from the same edge and go on to the same edge (a twist);
  - different-segment crossing: any other crossing;
  - separation: both come from and go on to the same edges, and are next to each other on one
    edge but not on the other.
- A line that ends at a node, or turns back at the end of an edge (out-and-back), makes no chord
  there. Stations (multipliers in the papers) are not part of the cost.

<img src="img/ordering-1.svg" width="240" alt="Two lines cross at a node exactly when their chords on a small circle around it interleave">

**Cost** = 3 × same-segment + 1 × different-segment + 2 × separations. Our own starting values:
a twist between lines that stay together looks worst; a crossing where lines part is often
forced; a gap opening between neighbors is in between. Tuned with the look.

<img src="img/ordering-2.svg" width="240" alt="The three kinds an order costs: a twist (3), a crossing where lines part (1), a separation (2)">

## Solving

Edges linked by a line passing from one to the other form independent parts. A part whose orders
number at most 10⁵ (the product of its edges' `n!`) gets every order tried, from the input order
on, keeping the first cheapest (ties keep the input order, so results are stable).

Larger parts (each city is one): a greedy descent, simulated annealing, then a last descent.
A move swaps two lines of one edge, either there only or on **their whole shared stretch** (every
edge both run through together). The second kind matters: two lines running together can only
change sides together; one edge at a time, each swap first makes a twist (cost 3), a wall the
search rarely climbs (found by the property test). A move is costed only on the pairs it changes
at the nodes it touches (exact: a property test compares it with the full cost). Temperature
falls from 3 to 0.01; the best order seen is kept. The budget is **steps, not time**:
`200 × line slots` within [5 000, 100 000] from a fixed seed, so every machine gets the same order
(on a real 50-route city, a 600–930-slot part, that is the full 100 000; only a cold solve
anneals, edits are warm). The later
optimizations (chain contraction, partner-line pruning, untangling rules) must give the same
results with fewer states (chains, below).

<img src="img/ordering-3.svg" width="240" alt="The search: every order for a small part, else greedy descent, simulated annealing and a last descent">

On real city networks of 50 lines (not in this repo), the search roughly halves the input order's
cost; ordering takes 40–350 ms natively, depending on the city.

## The first draw's budget

The budget is 200 steps per line slot, at most 100 000: a longer first draw buys a clearly lower
cost on city-sized parts (about 20–25% below the old 20 000-step budget). Chains of pass-through
edges share one order, which shrinks the search without changing its result. Partner lines
(identical walks; exact by the papers' Lemma 4.1) are left out: too few in real networks to gain
anything measurable. The untangling rules and the single-line cut are left out too: their
structures are rare in our graphs and the search settles them locally.

## Edits: warm start

After an edit, `solve_warm` starts from the order before it instead of from scratch, so the lines a user did not touch stay where they were (rule Q5) and the big part is not
annealed again:

<img src="img/ordering-4.svg" width="240" alt="After an edit, only the parts with a touched edge are searched again; the rest keep their order">

- **Warm start.** An edge with an exact twin before the edit (the same path and the same
  routes, renumbered) takes its old order. Another edge takes the order of the nearest old
  edge within 5 m of its middle with the same routes once the edit's own are set aside
  (changed or added ones here, removed ones there), turned round if it runs the other way; the
  edit's own lines go on its left for the search to place. Otherwise the input order. An edited
  route counts as the same route as before (same place in the list, same count).
- **Touched edges** have no exact twin or carry a changed or added route. Only they may change.
- **Parts** with no touched edge keep their order and are not solved again. A part
  with one is solved again with its untouched edges frozen: every order of the touched ones when
  there are at most 100 000 (judged at their own nodes), else greedy descent (moves on frozen edges
  are never made, also within a shared-stretch swap).

On real city networks, a 2 m nudge, a removed route or an added one costs within a few percent of
a cold solve, in a fraction of its time, and changes no order on edges the edit didn't touch. The
order now depends on the order before the edit; it is deterministic.

## How it relates to the papers

The model is the papers' line ordering with separations [BBS18, BBS19]: one order per edge,
crossings at nodes counted by kind (same-segment and different-segment), separations of lines
that run together, each weighted, and the total made as small as possible. The papers then
shrink the problem with reduction rules and solve what is left exactly. Where ours differs, and
why:

| The papers [BBS18, BBS19, B22]                                                                                                    | Ours                                                                                                                                                       | Why                                                                                                                   |
| --------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------- |
| An exact optimum from an integer linear program (ILP)                                                                             | Every order for a part with at most 10⁵ of them; above that, greedy descent, simulated annealing from a fixed seed with a step budget, then a last descent | No ILP solver in a browser package; a fixed number of steps bounds the time and gives the same order on every machine |
| Weights by kind, higher where a crossing falls at a station                                                                       | Fixed weights 3 / 1 / 2; stations are not in the cost                                                                                                      | Our own starting values, tuned with the look; stations come from the caller and are often absent                      |
| Reductions before solving [BBS19 §4]: partner lines (Lemma 4.1), node contraction (Lemma 4.2), single-line cuts, untangling rules | Node contraction only: chains of pass-through edges share one order                                                                                        | Measured on real networks: the other structures are rare, and the search settles them locally                         |
| A map solved once, from scratch                                                                                                   | After an edit, untouched edges keep their order and only touched parts are searched again                                                                  | An editor needs fast edits and lanes that don't jump                                                                  |

The cost model follows the papers; the search, its budget and the warm start after an edit are
ours. Without an exact solver, a large part's order is good, not proven optimal (the tests below
bound how far annealing ends from the exhaustive optimum).

## Tests

Annealing alone (exhaustive search off) ends within 10% of the exhaustive optimum on random
corridors of 3–4 stops (property test), reaches every hand-computed optimum below, and the same
seed gives the same order.

Hand-computed optima matched, in `ordering/`'s tests:

| Case              | Hand-computed                                                                                    |
| ----------------- | ------------------------------------------------------------------------------------------------ |
| Y (trunk, 3 ways) | input order costs 2 different-segment crossings; optimum: each line on the side of its branch, 0 |
| X (shared middle) | 1 crossing whatever the order (they swap sides); the tie keeps the input order                   |
| dog-bone          | input order with the north line on the south side: 2 crossings; optimum 0                        |
| twist (two edges) | swapping sides between two edges: 1 same-segment crossing (cost 3)                               |
| squeezed line     | a joining line between two neighbors: 1 separation + 1 crossing; optimum 0                       |

The synthetic fixtures all reach cost 0 (exhaustive search).
