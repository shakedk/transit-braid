// Vite's import suffixes used only by the inline entry (`inline.ts`, built by `vite.config.ts`).

/** `?worker&inline`: a worker whose bundled script is inside the importing file (Blob URL). */
declare module '*?worker&inline' {
  const InlineWorker: new () => Worker;
  export default InlineWorker;
}

/** `?inline`: the file as a `data:...;base64,` URL inside the importing file. */
declare module '*?inline' {
  const dataUrl: string;
  export default dataUrl;
}
