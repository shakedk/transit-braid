// The public names every entry shares: `transit-braid`, `transit-braid/node` and
// `transit-braid/inline` each export this list, plus their own `sideBySide()`, `createSession()`
// and `createBraidWorker()` (the same calls; only where the work runs differs). Anything not listed
// here is internal, so it may change in any release.

export type { BraidWorker } from './connection.ts';
export {
  BraidError,
  type ErrorCode,
  InputError,
  type InputErrorCode,
  type InputErrorDetails,
  InternalError,
  type InternalErrorCode,
} from './errors.ts';
export { DEFAULT_LINE_WIDTH, DEFAULT_SPACING, type SideBySideOptions } from './options.ts';
export type { BraidSession } from './session.ts';
export type {
  LineCollection,
  LineFeature,
  Position,
  RouteId,
  RoutesInput,
  SideBySideResult,
} from './routes.ts';
export type { StationBar, StationBars, StationsInput } from './stations.ts';
