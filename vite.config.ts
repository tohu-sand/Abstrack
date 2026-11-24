import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// The built page carries its script and its style sheet in itself. Each file
// of its own is one more request for the application to answer while the
// window starts, and the page waits for all of them before it runs.
const singleFile: Plugin = {
  name: "single-file",
  apply: "build",
  enforce: "post",
  generateBundle(_, bundle) {
    for (const page of Object.values(bundle)) {
      if (page.type !== "asset" || !page.fileName.endsWith(".html")) continue;
      let html = String(page.source);
      for (const [name, file] of Object.entries(bundle)) {
        const tag = (pattern: string) => new RegExp(`<${pattern}="/${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}"[^>]*>(</script>)?`);
        if (file.type === "chunk" && tag("script[^>]*src").test(html)) {
          // The end tag of a script cannot appear inside it, even in a string.
          html = html.replace(tag("script[^>]*src"), () => `<script type="module">${file.code.replace(/<\/script/g, "<\\/script")}</script>`);
          delete bundle[name];
        } else if (file.type === "asset" && tag("link[^>]*href").test(html)) {
          html = html.replace(tag("link[^>]*href"), () => `<style>${String(file.source)}</style>`);
          delete bundle[name];
        }
      }
      page.source = html;
    }
  },
};

export default defineConfig({
  plugins: [svelte(), singleFile],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**", "**/target/**", "**/ref/**", "**/work/**"] },
  },
  build: {
    target: "es2022",
    // There is one chunk: nothing is preloaded, and the polyfill would watch
    // every change to the page for links to preload.
    modulePreload: { polyfill: false },
    outDir: "dist",
    emptyOutDir: true,
  },
});
