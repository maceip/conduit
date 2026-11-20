// Renders docs/diagrams/*.mmd to SVG for the README on GitHub.
// The pages site renders the same sources to text with mermaid-ascii.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { renderMermaidSVG } from 'beautiful-mermaid'

const dir = join(dirname(fileURLToPath(import.meta.url)), '../../docs/diagrams')
// Light grey, not white or black, so the pictures read on GitHub in both
// light and dark mode; the corners are cut round by the background rect.
const bg = '#E4E4E7'
const theme = { bg, fg: '#18181B', line: '#27272A', border: '#3F3F46', surface: '#F4F4F5', muted: '#27272A', transparent: true }
const font = 'ui-monospace, Menlo, Consolas, "Liberation Mono", "Courier New", monospace'

for (const name of readdirSync(dir).filter((f) => f.endsWith('.mmd'))) {
  // The renderer quotes the whole stack as one family and imports it from
  // Google Fonts; drop the import and use the stack as a real fallback list.
  const svg = renderMermaidSVG(readFileSync(join(dir, name), 'utf8'), { ...theme, font: 'Menlo' })
    .replace(/^\s*@import url\([^)]*\);\n/m, '')
    .replace(/font-family: 'Menlo', system-ui, sans-serif;/, `font-family: ${font};`)
    // The layout measures text as if it were proportional, and monospace runs
    // about a quarter wider. Shrink the text to fit its boxes, then draw the
    // whole picture a quarter larger so the text is back to its normal size.
    .replace(/font-size="([\d.]+)"/g, (_, n) => `font-size="${(n * 0.8).toFixed(1)}"`)
    // Heavier lines and text, but keep marker arrowheads crisp and drop the
    // fake box border on edge labels so labels read as wire annotations.
    .replace(/(<(?:polyline|line|path|rect)\b[^>]*?)stroke-width="([\d.]+)"/g, (_, pre, n) => `${pre}stroke-width="${(n * 1.75).toFixed(2)}"`)
    .replace(/(<g class="edge-label"[^>]*>\s*<rect\b[^>]*?)stroke="[^"]*" stroke-width="[^"]*"/g, '$1stroke="none"')
    .replace(/font-weight="400"/g, 'font-weight="600"')
    .replace(/font-weight="500"/g, 'font-weight="700"')
    .replace('</style>', `</style>\n<rect x="0" y="0" width="100%" height="100%" rx="18" ry="18" fill="${bg}"/>`)
    .replace(/^<svg([^>]*) width="([\d.]+)" height="([\d.]+)"/, (_, a, w, h) => `<svg${a} width="${(w * 1.25).toFixed(0)}" height="${(h * 1.25).toFixed(0)}"`)
  writeFileSync(join(dir, name.replace(/\.mmd$/, '.svg')), svg)
  console.log('ok', name)
}
