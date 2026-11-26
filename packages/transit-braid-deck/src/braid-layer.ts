// transit-braid-deck: a deck.gl layer that draws overlapping routes side by side with
// transit-braid's `sideBySide()`, plus station bars.

import {
  CompositeLayer,
  type Color,
  type CompositeLayerProps,
  type DefaultProps,
  type Layer,
  type UpdateParameters,
} from '@deck.gl/core';
import { PathLayer } from '@deck.gl/layers';
import {
  type BraidSession,
  createSession,
  type LineFeature,
  type RouteId,
  type RoutesInput,
  type SideBySideResult,
  type StationBar,
  type StationsInput,
} from 'transit-braid';

import { colorOf } from './color.ts';

/** How long the zoom must stay still before the lines are computed again for it, in ms. */
export const ZOOM_SETTLE_MS = 150;

/** One route as `data` gives it (a GeoJSON LineString feature with its id). */
export type BraidRoute = RoutesInput['features'][number];

/** BraidLayer's own props (deck.gl's layer props come on top). */
export interface BraidLayerOwnProps {
  /** The routes: the same GeoJSON FeatureCollection `sideBySide()` takes. */
  data: RoutesInput;
  /** Stations to draw bars for, as `sideBySide()`'s `stations` option; none when left out. */
  stations?: StationsInput | null;
  /** Each route's color; default: its `properties.color` (`#rrggbb`), else gray. */
  getColor?: (route: BraidRoute) => Color;
  /** Line width in pixels (as `sideBySide()`'s `lineWidth`). */
  lineWidth?: number;
  /** Gap between neighboring lines in pixels (as `sideBySide()`'s `spacing`). */
  spacing?: number;
  /** Lane cap, as `sideBySide()`'s `maxSideBySide`: routes over it are left out whole;
   * none when left out or `null`. */
  maxSideBySide?: number | null;
  /** Called with the ids of the routes the cap left out (`sideBySide()`'s `dropped`, in input
   * order): once for the first result drawn, then whenever that list changes. */
  onDropped?: ((dropped: RouteId[]) => void) | null;
}

/** All of BraidLayer's props. */
export type BraidLayerProps = BraidLayerOwnProps & CompositeLayerProps;

interface BraidState {
  /** This layer's session: its memory makes a zoom change redraw only the geometry, and an edit
   * keep the lanes it had. Closed when the layer is removed. */
  session: BraidSession;
  /** The last good result, drawn until a newer one arrives (or nothing yet). */
  result: SideBySideResult | null;
  /** The zoom the shown (or computing) result is for. */
  zoom: number | null;
  /** Calls asked for so far; an answer for an older one is dropped. */
  asked: number;
  /** A call is under way. */
  busy: boolean;
  /** The pending zoom-settle timer. */
  timer: ReturnType<typeof setTimeout> | null;
  /** The layer was removed: late answers are ignored. */
  finalized: boolean;
  /** The dropped routes last given to `onDropped`, as JSON (null: none given yet). */
  reported: string | null;
  [key: string]: unknown;
}

/** GeoJSON positions as deck.gl's `[lng, lat]` points. */
function toPath(coordinates: readonly (readonly number[])[]): [number, number][] {
  return coordinates.map((p): [number, number] => [p[0] ?? 0, p[1] ?? 0]);
}

const defaultProps: DefaultProps<BraidLayerProps> = {
  stations: null,
  getColor: { type: 'accessor', value: colorOf },
  lineWidth: { type: 'number', value: 4, min: 0 },
  spacing: { type: 'number', value: 1, min: 0 },
  maxSideBySide: null,
  onDropped: { type: 'function', value: null, optional: true },
};

/**
 * Draws `data`'s routes side by side (one line per route, in its own slot) and a bar across the
 * bundle for each station. It owns a `createSession()` session for as long as it is on the map,
 * and calls it when its props change and when the zoom has stayed still for [`ZOOM_SETTLE_MS`];
 * meanwhile the last good result stays on screen, and only the newest request's answer is drawn.
 * A failed call keeps the last good result and reports the error to deck.gl (`onError`). Picking a
 * line gives its route: the output feature, with the route's id and properties.
 */
export class BraidLayer extends CompositeLayer<Required<BraidLayerOwnProps>> {
  static override layerName = 'BraidLayer';
  static override defaultProps: DefaultProps<BraidLayerProps> = defaultProps;

  declare state: BraidState;

  override initializeState(): void {
    this.state = {
      session: createSession(),
      result: null,
      zoom: null,
      asked: 0,
      busy: false,
      timer: null,
      finalized: false,
      reported: null,
    };
  }

  override shouldUpdateState({ changeFlags }: UpdateParameters<this>): boolean {
    return changeFlags.somethingChanged;
  }

  override updateState({ props, oldProps, changeFlags }: UpdateParameters<this>): void {
    const zoom = this.context.viewport.zoom;
    const inputChanged =
      changeFlags.dataChanged !== false ||
      props.stations !== oldProps.stations ||
      props.lineWidth !== oldProps.lineWidth ||
      props.spacing !== oldProps.spacing ||
      props.maxSideBySide !== oldProps.maxSideBySide;
    if (inputChanged || this.state.zoom === null) {
      this.compute(zoom);
    } else if (changeFlags.viewportChanged && zoom !== this.state.zoom) {
      // Zooming: wait until it settles, keeping the last result on screen meanwhile.
      if (this.state.timer !== null) clearTimeout(this.state.timer);
      this.state.timer = setTimeout(() => {
        const layer = this.getCurrentLayer();
        if (layer instanceof BraidLayer && !layer.state.finalized) {
          layer.state.timer = null;
          layer.compute(layer.context.viewport.zoom);
        }
      }, ZOOM_SETTLE_MS);
    }
  }

  override finalizeState(): void {
    this.state.finalized = true;
    if (this.state.timer !== null) clearTimeout(this.state.timer);
    this.state.session.close();
  }

  /** Asks for the lines at `zoom` (deck.gl's, in 512-pixel tiles like `sideBySide()`'s); only
   * the newest request's answer is kept. */
  private compute(zoom: number): void {
    const state = this.state;
    state.asked += 1;
    state.zoom = zoom;
    if (state.busy) return; // the call under way runs the newest request when it ends
    void this.run();
  }

  /** Runs calls one at a time until the newest request is answered: requests made while a call
   * runs collapse into the newest one. */
  private async run(): Promise<void> {
    const state = this.state;
    state.busy = true;
    let handled = 0;
    while (handled !== state.asked && !state.finalized) {
      handled = state.asked;
      const layer = this.getCurrentLayer();
      if (!(layer instanceof BraidLayer)) break;
      const { data, stations, lineWidth, spacing, maxSideBySide } = layer.props;
      const zoom = state.zoom ?? layer.context.viewport.zoom;
      try {
        // oxlint-disable-next-line eslint/no-await-in-loop -- one call at a time is the point
        const result = await state.session.sideBySide(data, {
          zoom,
          lineWidth,
          spacing,
          maxSideBySide,
          ...(stations ? { stations } : {}),
        });
        const current = this.getCurrentLayer();
        if (handled === state.asked && !state.finalized && current instanceof BraidLayer) {
          current.setState({ result });
          const reported = JSON.stringify(result.dropped);
          if (reported !== state.reported) {
            state.reported = reported;
            current.props.onDropped?.(result.dropped);
          }
        }
      } catch (error) {
        const current = this.getCurrentLayer();
        if (handled === state.asked && !state.finalized && current instanceof BraidLayer) {
          current.raiseError(
            error instanceof Error ? error : new Error(String(error)),
            'BraidLayer',
          );
        }
      }
    }
    state.busy = false;
  }

  override renderLayers(): Layer[] {
    const result = this.state.result;
    if (!result) return [];
    const { getColor, lineWidth, data } = this.props;
    const routeById = new Map<unknown, BraidRoute>(data.features.map((f) => [f.id, f]));
    return [
      new PathLayer<LineFeature>(
        this.getSubLayerProps({
          id: 'lines',
          updateTriggers: { getColor: [getColor, data] },
        }),
        {
          data: result.lines.features,
          getPath: (f: LineFeature) => toPath(f.geometry.coordinates),
          getColor: (f: LineFeature) => {
            const route = routeById.get(f.id);
            return route ? getColor(route) : colorOf(f);
          },
          getWidth: lineWidth,
          widthUnits: 'pixels',
          capRounded: true,
          jointRounded: true,
          pickable: this.props.pickable,
        },
      ),
      new PathLayer<StationBar>(this.getSubLayerProps({ id: 'stations' }), {
        data: result.stations.features,
        getPath: (f: StationBar) => toPath(f.geometry.coordinates),
        getColor: [40, 40, 40],
        getWidth: Math.max(2, lineWidth / 2),
        widthUnits: 'pixels',
        capRounded: true,
        pickable: false,
      }),
    ];
  }
}
