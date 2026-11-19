// Renders docs/diagrams/*.mmd to SVG for the README on GitHub.
// The pages site renders the same sources to text with mermaid-ascii.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { renderMermaidSVG, THEMES } from 'beautiful-mermaid'

const dir = join(dirname(fileURLToPath(import.meta.url)), '../../docs/diagrams')
const font = 'ui-monospace, Menlo, Consolas, "Liberation Mono", "Courier New", monospace'

for (const name of readdirSync(dir).filter((f) => f.endsWith('.mmd'))) {
  // The renderer quotes the whole stack as one family and imports it from
  // Google Fonts; drop the import and use the stack as a real fallback list.
  const svg = renderMermaidSVG(readFileSync(join(dir, name), 'utf8'), { ...THEMES['zinc-light'], font: 'Menlo' })
    .replace(/^\s*@import url\([^)]*\);\n/m, '')
    .replace(/font-family: 'Menlo', system-ui, sans-serif;/, `font-family: ${font};`)
    // The layout measures text as if it were proportional, and monospace runs
    // about a quarter wider. Shrink the text to fit its boxes, then draw the
    // whole picture a quarter larger so the text is back to its normal size.
    .replace(/font-size="([\d.]+)"/g, (_, n) => `font-size="${(n * 0.8).toFixed(1)}"`)
    .replace(/^<svg([^>]*) width="([\d.]+)" height="([\d.]+)"/, (_, a, w, h) => `<svg${a} width="${(w * 1.25).toFixed(0)}" height="${(h * 1.25).toFixed(0)}"`)
  writeFileSync(join(dir, name.replace(/\.mmd$/, '.svg')), svg)
  console.log('ok', name)
}
