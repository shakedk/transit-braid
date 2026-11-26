# transit-braid-deck

A [deck.gl](https://deck.gl) layer that draws overlapping transit routes side by side, with
[`transit-braid`](https://www.npmjs.com/package/transit-braid): each route in its own lane where
routes share a street, plus station bars across each bundle.

> **Status:** early development (0.x). The API may still change between minor versions.

## Install

```sh
npm install transit-braid-deck transit-braid @deck.gl/core @deck.gl/layers
```

`transit-braid` and deck.gl (9.4.1 or a later 9.x) are peer dependencies: your app installs them.

## Use

```ts
import { BraidLayer } from 'transit-braid-deck';

new BraidLayer({
  id: 'routes',
  data: routes, // a GeoJSON FeatureCollection of LineStrings, each with its own `id`
  stations, // optional: a FeatureCollection of Points
  lineWidth: 4,
  spacing: 1,
  pickable: true,
});
```

`data` is what `transit-braid`'s `sideBySide()` takes. The layer keeps a `transit-braid` session
while it is on the map: it draws again when its props change and once the zoom has stayed still
for 150 ms (`ZOOM_SETTLE_MS`), keeping the last good lines on screen meanwhile. A route's color is
`getColor(route)`, by default its `properties.color` (`#rrggbb`), else gray. `maxSideBySide` caps
the lanes, and `onDropped` gets the ids of the routes the cap left out. A failed call keeps the
last good lines and goes to deck.gl's `onError`. Picking a line gives its route: the output
feature, with the route's id and properties.

## License

[MIT](LICENSE). More in the [repository](https://github.com/shakedk/transit-braid).
