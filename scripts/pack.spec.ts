// The packages as npm would ship them. Both are packed (`pnpm pack`, which applies
// `publishConfig` and turns `workspace:` versions into real ones) and installed from the tarballs
// into a throwaway project outside this workspace, so nothing resolves through a workspace link.
// Then that project uses them as an app would: Node 24 runs `transit-braid/node`, tsc checks the
// installed types, and Vite bundles a page with the default entry, the inline entry and the
// deck.gl layer, which runs in Chromium. Needs `pnpm build:wasm && pnpm build:packages` first;
// run by `pnpm test:pack`.
import { execFile } from 'node:child_process';
import { mkdir, mkdtemp, readdir, readFile, rm, writeFile } from 'node:fs/promises';
import { createServer, type Server } from 'node:http';
import { homedir, tmpdir } from 'node:os';
import { extname, join, relative, resolve, sep } from 'node:path';
import { promisify } from 'node:util';

import { expect, test } from '@playwright/test';

import type * as Braid from '../packages/transit-braid/src/index.ts';

const exec = promisify(execFile);

const REPO = join(import.meta.dirname, '..');
const PACKAGES = join(REPO, 'packages');
const NAMES = ['transit-braid', 'transit-braid-deck'] as const;
type Name = (typeof NAMES)[number];

/** What `pnpm pack --json` reports for one package. */
interface Packed {
  name: string;
  version: string;
  filename: string;
  files: { path: string }[];
}

/** The fields of a packed `package.json` this test reads. */
interface Manifest {
  name: string;
  version: string;
  private?: boolean;
  exports: unknown;
  publishConfig?: unknown;
  engines?: unknown;
  dependencies?: Record<string, string>;
  peerDependencies?: Record<string, string>;
}

/** Everything each package may ship: every packed path matches one of these. */
const SHIPS: Record<Name, RegExp[]> = {
  'transit-braid': [
    /^(package\.json|README\.md|LICENSE)$/,
    /^dist\/[a-z-]+\.(js|d\.ts)$/,
    /^wasm\/transit_braid_wasm(\.js|\.d\.ts|_bg\.wasm)$/,
  ],
  'transit-braid-deck': [/^(package\.json|README\.md|LICENSE)$/, /^dist\/[a-z-]+\.(js|d\.ts)$/],
};

/** Two routes along one street: they must come out side by side. */
const LAT = 32.07;
const ROUTES: Braid.RoutesInput = {
  type: 'FeatureCollection',
  features: ['A', 'B'].map((id) => ({
    type: 'Feature' as const,
    id,
    properties: { color: id === 'A' ? '#ff0000' : null },
    geometry: {
      type: 'LineString' as const,
      coordinates: [
        [34.78, LAT],
        [34.785, LAT],
      ],
    },
  })),
};

/** The throwaway project, its tarballs and their unpacked copies, all in one temp folder. */
let work = '';
let app = '';
const packed = new Map<Name, Packed>();
const manifests = new Map<Name, Manifest>();

/** The version of `pkg` this workspace installed (`dir` is a package that depends on it). */
async function installed(dir: string, pkg: string): Promise<string> {
  const text = await readFile(join(dir, 'node_modules', pkg, 'package.json'), 'utf8');
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- an installed package's manifest
  return (JSON.parse(text) as { version: string }).version;
}

/** Every file under `dir`, as `/`-separated paths relative to it. */
async function filesUnder(dir: string): Promise<string[]> {
  const entries = await readdir(dir, { recursive: true, withFileTypes: true });
  return entries
    .filter((entry) => entry.isFile())
    .map((entry) => relative(dir, join(entry.parentPath, entry.name)).split(sep).join('/'))
    .toSorted();
}

/** Every path an `exports` map points at (each condition's target). */
function targets(exports: unknown): string[] {
  if (typeof exports === 'string') return [exports];
  if (typeof exports !== 'object' || exports === null) return [];
  return Object.values(exports).flatMap(targets);
}

/** Runs the repo's TypeScript on the project in `dir`; its errors fail the test. */
async function tsc(dir: string): Promise<void> {
  const bin = join(REPO, 'node_modules', 'typescript', 'bin', 'tsc');
  const output = await exec(process.execPath, [bin, '-p', dir]).then(
    () => '',
    // tsc prints its errors on stdout.
    (error: unknown) =>
      error instanceof Error && 'stdout' in error && typeof error.stdout === 'string'
        ? error.stdout
        : String(error),
  );
  expect(output, `tsc -p ${relative(app, dir)}`).toBe('');
}

test.beforeAll(async () => {
  test.setTimeout(180_000);
  work = await mkdtemp(join(tmpdir(), 'braid-pack-'));
  const tarballs = join(work, 'tarballs');
  await Promise.all(
    NAMES.map(async (name) => {
      const { stdout } = await exec('pnpm', ['pack', '--pack-destination', tarballs, '--json'], {
        cwd: join(PACKAGES, name),
      });
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- pnpm's own report
      const pack = JSON.parse(stdout) as Packed;
      packed.set(name, pack);
      const unpacked = join(work, 'unpacked', name);
      await mkdir(unpacked, { recursive: true });
      await exec('tar', ['-xzf', pack.filename, '-C', unpacked]);
      const manifest = await readFile(join(unpacked, 'package', 'package.json'), 'utf8');
      // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- read by the assertions
      manifests.set(name, JSON.parse(manifest) as Manifest);
    }),
  );

  // The app installs the tarballs, deck.gl (transit-braid-deck's peer) at the versions this
  // workspace uses, and Node's types for the Node type check.
  app = join(work, 'app');
  const deck = join(PACKAGES, 'transit-braid-deck');
  const dependencies: Record<string, string> = {
    '@deck.gl/core': await installed(deck, '@deck.gl/core'),
    '@deck.gl/layers': await installed(deck, '@deck.gl/layers'),
    '@types/node': await installed(REPO, '@types/node'),
  };
  for (const [name, pack] of packed) dependencies[name] = `file:${pack.filename}`;
  await mkdir(app);
  await writeFile(
    join(app, 'package.json'),
    JSON.stringify({ name: 'braid-pack-app', private: true, type: 'module', dependencies }),
  );
  // Same supply-chain rules as the workspace: no install scripts, no version under 7 days old,
  // no version published with less trust than the ones before it.
  await exec(
    'pnpm',
    [
      'install',
      '--prefer-offline',
      '--ignore-scripts',
      '--config.minimum-release-age=10080',
      '--config.trust-policy=no-downgrade',
    ],
    { cwd: app },
  );
});

test.afterAll(async () => {
  if (work !== '') await rm(work, { recursive: true, force: true });
});

/** One package's tarball: pnpm's list of its files, and each file's bytes as unpacked. */
async function tarball(name: Name): Promise<{ paths: string[]; bytes: Map<string, Buffer> }> {
  const pack = packed.get(name);
  if (!pack) throw new Error(`${name} was not packed`);
  const paths = pack.files.map((file) => file.path).toSorted();
  const unpacked = join(work, 'unpacked', name, 'package');
  // pnpm's list is the tarball's content.
  expect(await filesUnder(unpacked)).toEqual(paths);
  const read = await Promise.all(
    paths.map(async (path) => [path, await readFile(join(unpacked, path))] as const),
  );
  return { paths, bytes: new Map(read) };
}

test('each tarball holds only what ships, and its manifest is the published one', async () => {
  const tarballs = await Promise.all(NAMES.map(tarball));
  for (const [i, name] of NAMES.entries()) {
    const manifest = manifests.get(name);
    const { paths, bytes } = tarballs[i] ?? { paths: [], bytes: new Map<string, Buffer>() };
    if (!manifest) throw new Error(`${name} was not packed`);
    expect(paths.filter((path) => !SHIPS[name].some((ships) => ships.test(path)))).toEqual([]);
    expect(paths).toEqual(expect.arrayContaining(['LICENSE', 'README.md', 'package.json']));
    expect(paths.filter((path) => /test|\.map$|^src\/|fixture/.test(path))).toEqual([]);

    // Every export points into the tarball (dist/, never the TypeScript source).
    const exported = targets(manifest.exports);
    expect(exported.length).toBeGreaterThan(1);
    for (const target of exported) expect(paths).toContain(target.replace(/^\.\//, ''));
    expect(JSON.stringify(manifest)).not.toContain('workspace:');
    // Publishable: not private, public access, provenance when the release workflow publishes.
    expect(manifest.private).toBeUndefined();
    expect(manifest.publishConfig).toEqual({ access: 'public', provenance: true });
    expect(manifest.engines).toEqual({ node: '>=24' });

    // No file holds a path from the machine that built it.
    for (const [path, content] of bytes) {
      for (const local of [REPO, homedir()]) {
        expect(content.includes(local), `${name}/${path} holds ${local}`).toBe(false);
      }
    }
  }
  const core = manifests.get('transit-braid');
  const deck = manifests.get('transit-braid-deck');
  expect(core?.dependencies).toBeUndefined();
  expect(deck?.dependencies).toBeUndefined();
  expect(deck?.peerDependencies?.['transit-braid']).toBe(`^${core?.version ?? '?'}`);
});

test('Node 24 runs transit-braid/node from the installed package, and exits by itself', async () => {
  await writeFile(
    join(app, 'node-check.js'),
    `import * as node from 'transit-braid/node';
import * as main from 'transit-braid';
import * as inline from 'transit-braid/inline';

const routes = JSON.parse(process.argv[2]);
const pure = await node.sideBySide(routes, { zoom: 16 });
const session = node.createSession();
const first = await session.sideBySide(routes, { zoom: 16 });
const zoomed = await session.sideBySide(routes, { zoom: 17 });
session.close();
const closed = await session.sideBySide(routes, { zoom: 16 }).then(
  () => 'drawn',
  (e) => (e instanceof node.BraidError ? e.code : String(e)),
);
const worker = node.createBraidWorker();
const onWorker = await worker.sideBySide(routes, { zoom: 16 });
worker.terminate();
const refused = await node.sideBySide(null, { zoom: 16 }).then(
  () => 'drawn',
  (e) => (e instanceof node.InputError ? e.code : String(e)),
);
console.log(JSON.stringify({
  ids: pure.lines.features.map((f) => f.id),
  north: pure.lines.features.map((f) => f.geometry.coordinates[0][1] - ${String(LAT)}),
  session: JSON.stringify(first) === JSON.stringify(pure),
  zoomed: zoomed.lines.features.length,
  closed,
  worker: JSON.stringify(onWorker) === JSON.stringify(pure),
  refused,
  // Under Node, transit-braid and transit-braid/inline are the Node entry too.
  oneEntry: main.sideBySide === node.sideBySide && inline.sideBySide === node.sideBySide,
}));
`,
  );
  const { stdout } = await exec(process.execPath, ['node-check.js', JSON.stringify(ROUTES)], {
    cwd: app,
    timeout: 60_000,
  });
  // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- the script's own report, checked below
  const { north, ...out } = JSON.parse(stdout) as { north: number[] } & Record<string, unknown>;
  expect(out).toEqual({
    ids: ['A', 'B'],
    session: true,
    zoomed: 2,
    closed: 'CLOSED',
    worker: true,
    refused: 'BAD_ROUTES',
    oneEntry: true,
  });
  // One street, two lanes: one line each side of it.
  const [a = 0, b = 0] = north;
  expect(a).toBeCloseTo(-b, 12);
  expect(Math.abs(a)).toBeGreaterThan(0);
});

test('the installed types check in a browser app and a Node app, without skipLibCheck', async () => {
  const projects: Record<string, { compilerOptions: object; code: string }> = {
    web: {
      compilerOptions: {
        lib: ['es2023', 'dom'],
        module: 'esnext',
        moduleResolution: 'bundler',
        types: [],
        skipLibCheck: false,
      },
      code: `import { BraidError, createSession, InputError, sideBySide, type RoutesInput, type SideBySideResult } from 'transit-braid';
import * as inline from 'transit-braid/inline';

const routes: RoutesInput = { type: 'FeatureCollection', features: [] };
export const drawn: Promise<SideBySideResult> = sideBySide(routes, { zoom: 14 });
export const inlined: Promise<SideBySideResult> = inline.sideBySide(routes, { metersPerPixel: 2 });
createSession().close();
// @ts-expect-error a zoom is a number: the types are real, not \`any\`
void sideBySide(routes, { zoom: '14' });
export const isInput = (e: unknown): boolean => e instanceof InputError && e instanceof BraidError && e.code === 'BAD_ROUTES';
`,
    },
    node: {
      compilerOptions: {
        lib: ['es2023'],
        module: 'nodenext',
        moduleResolution: 'nodenext',
        types: ['node'],
        skipLibCheck: false,
      },
      code: `import { createBraidWorker, createSession, InputError, sideBySide, type RoutesInput, type SideBySideResult } from 'transit-braid/node';
import * as main from 'transit-braid';

const routes: RoutesInput = { type: 'FeatureCollection', features: [] };
export const drawn: Promise<SideBySideResult> = sideBySide(routes, { zoom: 14 });
export const same: Promise<SideBySideResult> = main.sideBySide(routes, { zoom: 14 });
createSession().close();
createBraidWorker().terminate();
// @ts-expect-error a zoom is a number: the types are real, not \`any\`
void sideBySide(routes, { zoom: '14' });
export const isInput = (e: unknown): boolean => e instanceof InputError && e.code === 'TOO_MANY_ROUTES';
`,
    },
    // deck.gl's own types are deck.gl's business, so this one skips checking library files.
    deck: {
      compilerOptions: {
        lib: ['es2023', 'dom'],
        module: 'esnext',
        moduleResolution: 'bundler',
        types: [],
        skipLibCheck: true,
      },
      code: `import type { RoutesInput } from 'transit-braid';
import { BraidLayer, type BraidLayerProps } from 'transit-braid-deck';

const data: RoutesInput = { type: 'FeatureCollection', features: [] };
const props: BraidLayerProps = { id: 'routes', data, lineWidth: 4 };
export const layer = new BraidLayer(props);
// @ts-expect-error data is routes: the types are real, not \`any\`
export const wrong = new BraidLayer({ id: 'routes', data: 42 });
`,
    },
  };
  await Promise.all(
    Object.entries(projects).map(async ([name, project]) => {
      const dir = join(app, 'types', name);
      await mkdir(dir, { recursive: true });
      await writeFile(join(dir, 'check.ts'), project.code);
      await writeFile(
        join(dir, 'tsconfig.json'),
        JSON.stringify({
          compilerOptions: {
            target: 'es2023',
            strict: true,
            noEmit: true,
            ...project.compilerOptions,
          },
          files: ['check.ts'],
        }),
      );
      await tsc(dir);
    }),
  );
});

declare global {
  interface Window {
    braidPack?: {
      main: typeof Braid;
      inline: typeof Braid;
      BraidLayer: (new (props: { id: string; data: Braid.RoutesInput }) => { id: string }) & {
        layerName: string;
      };
    };
  }
}

/** Serves `root`'s files on a free local port; `served` lists each path asked for, with its status. */
async function serve(root: string): Promise<{ server: Server; origin: string; served: string[] }> {
  const types: Record<string, string> = {
    '.html': 'text/html',
    '.js': 'text/javascript',
    '.wasm': 'application/wasm',
  };
  const served: string[] = [];
  const server = createServer((request, response) => {
    const path = decodeURIComponent(new URL(request.url ?? '/', 'http://x').pathname);
    const file = resolve(root, `.${path === '/' ? '/index.html' : path}`);
    const missing = (): void => {
      served.push(`404 ${path}`);
      response.writeHead(404).end();
    };
    // Only files inside `root`.
    if (!file.startsWith(root + sep)) {
      missing();
      return;
    }
    readFile(file).then((body) => {
      served.push(`200 ${path}`);
      response.writeHead(200, { 'content-type': types[extname(file)] ?? 'text/plain' });
      response.end(body);
    }, missing);
  });
  await new Promise<void>((ready) => {
    server.listen(0, '127.0.0.1', ready);
  });
  const address = server.address();
  const port = typeof address === 'object' && address !== null ? address.port : 0;
  return { server, origin: `http://127.0.0.1:${String(port)}`, served };
}

test('a page bundled by Vite runs the default and inline entries and the deck.gl layer', async ({
  page,
}) => {
  const site = join(app, 'page');
  await mkdir(site, { recursive: true });
  await writeFile(
    join(site, 'index.html'),
    `<!doctype html>
<meta charset="utf-8" />
<title>pack test</title>
<script type="module" src="./main.js"></script>
`,
  );
  await writeFile(
    join(site, 'main.js'),
    `import * as main from 'transit-braid';
import * as inline from 'transit-braid/inline';
import { BraidLayer } from 'transit-braid-deck';

window.braidPack = { main, inline, BraidLayer };
`,
  );
  // This workspace's Vite (transit-braid's own build tool), run on the app's folder: it
  // resolves the packages from the app's node_modules.
  const vite = join(PACKAGES, 'transit-braid', 'node_modules', 'vite', 'bin', 'vite.js');
  await exec(process.execPath, [vite, 'build', site, '--logLevel', 'error']);
  const built = await filesUnder(join(site, 'dist'));
  // The default entry's worker script and .wasm became files of their own.
  const worker = built.filter((file) => /^assets\/worker-[\w-]+\.js$/.test(file));
  const wasm = built.filter((file) => /^assets\/transit_braid_wasm_bg-[\w-]+\.wasm$/.test(file));
  expect(worker).toHaveLength(1);
  expect(wasm).toHaveLength(1);

  const { server, origin, served } = await serve(join(site, 'dist'));
  try {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    page.on('console', (message) => {
      if (message.type() === 'error') errors.push(message.text());
    });
    await page.goto(`${origin}/`);
    await expect.poll(() => page.evaluate(() => typeof window.braidPack)).toBe('object');
    const out = await page.evaluate(async (routes) => {
      const pack = window.braidPack;
      if (!pack) throw new Error('no braidPack');
      const draws = await Promise.all(
        [pack.main, pack.inline].map(async (entry) => {
          const pure = await entry.sideBySide(routes, { zoom: 16 });
          const session = entry.createSession();
          const first = await session.sideBySide(routes, { zoom: 16 });
          session.close();
          const refused = await entry
            // oxlint-disable-next-line typescript/no-unsafe-type-assertion -- deliberately wrong input
            .sideBySide(null as unknown as typeof routes, { zoom: 16 })
            .then(
              () => 'drawn',
              (e: unknown) => (e instanceof entry.InputError ? e.code : String(e)),
            );
          return {
            ids: pure.lines.features.map((f) => f.id),
            pure: JSON.stringify(pure),
            session: JSON.stringify(first),
            refused,
          };
        }),
      );
      const layer = new pack.BraidLayer({ id: 'routes', data: routes });
      return { draws, layer: { id: layer.id, name: pack.BraidLayer.layerName } };
    }, ROUTES);
    const [main, inline] = out.draws;
    expect([main?.ids, inline?.ids]).toEqual([
      ['A', 'B'],
      ['A', 'B'],
    ]);
    // Both entries, pure and in a session, draw the same lines; both refuse bad input.
    expect(main?.session).toBe(main?.pure);
    expect(inline?.pure).toBe(main?.pure);
    expect(inline?.session).toBe(main?.pure);
    expect([main?.refused, inline?.refused]).toEqual(['BAD_ROUTES', 'BAD_ROUTES']);
    expect(out.layer).toEqual({ id: 'routes', name: 'BraidLayer' });
    // The default entry loaded its worker and .wasm from the built files; nothing was missing.
    expect(served).toEqual(
      expect.arrayContaining([`200 /${worker[0] ?? ''}`, `200 /${wasm[0] ?? ''}`]),
    );
    // (The browser's own favicon request is the only one allowed to miss.)
    expect(served.filter((line) => !/^200 |^404 \/favicon\.ico$/.test(line))).toEqual([]);
    expect(errors).toEqual([]);
  } finally {
    server.close();
  }
});
