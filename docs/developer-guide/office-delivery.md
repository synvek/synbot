# Office deliverables

Agents write documents as files — `report.docx`, `budget.xlsx`, `deck.pptx`, `chart.pdf`. The **Deliverables** tab of the workspace panel turns those files into something you can read in the dashboard instead of downloading a binary and guessing what is inside.

Nothing leaves the machine: synbot reads the OOXML package itself (a `.docx` is a ZIP of XML parts), extracts the text, and lets the browser render the rest. There is no LibreOffice, no conversion service and no extra dependency.

## What is listed

The tab lists every file in the project whose extension maps to a kind:

| Kind | Extensions |
|------|------------|
| `docx` | `docx` |
| `xlsx` | `xlsx`, `xlsm` |
| `pptx` | `pptx` |
| `pdf` | `pdf` |
| `csv` | `csv`, `tsv` |
| `markdown` | `md`, `markdown` |
| `html` | `html`, `htm` |
| `image` | `png`, `jpg`, `jpeg`, `gif`, `webp`, `svg`, `bmp` |
| `text` | `txt`, `log`, `json`, `yml`, `yaml`, `toml` |

Everything else (source code, archives, legacy `.doc`/`.xls`/`.ppt`) is *not* a deliverable and stays in the Files tab.

The listing walks the project recursively and:

- skips hidden directories and build noise (`.git`, `node_modules`, `target`, `dist`, `build`, …);
- skips symbolic links — a link out of the project must not show up as a deliverable;
- stops after 500 documents, 20 000 inspected entries or 12 directory levels, and flags `truncated`;
- returns the newest document first, so the file the agent just wrote is on top.

## Preview

Clicking a row opens a preview; every row (and the preview header) has an **Export** button that downloads the original bytes.

| Kind | What the preview shows |
|------|------------------------|
| `docx` | Title/author from the document properties, word and paragraph counts, then the paragraphs in document order. Tables read as one paragraph per cell. |
| `xlsx` | One table per sheet, with sheet tabs, row numbers and the used cells as text. |
| `pptx` | One card per slide: the first text line as the title, the remaining lines as bullets. |
| `pdf`, `image` | The browser renders the bytes (the server only serves them). |
| `csv`, `markdown`, `html`, `text` | The text itself, capped at 40 000 characters. |

Values are shown the way they are stored: a date cell shows its serial number, a formula shows the value Excel cached, and merged cells repeat nothing. Formatting, fonts, images, charts and layout are out of scope — see [Boundaries](#boundaries).

## Limits

Caps apply while reading, so one hostile or enormous file cannot stall the daemon:

| Cap | Value |
|-----|-------|
| File size opened at all | 64 MB |
| Single inflated ZIP entry | 32 MB |
| Characters of plain text | 40 000 |
| Paragraphs (`docx`) | 300 |
| Sheets (`xlsx`) / rows per sheet / columns per row | 20 / 200 / 40 |
| Shared strings (`xlsx`) | 5 000 |
| Slides (`pptx`) / lines per slide | 60 / 40 |
| Download through the dashboard | 64 MB |

When a cap is hit the preview says so and the export still gives you the whole file.

## HTTP API

| Endpoint | Result |
|----------|--------|
| `GET /api/projects/{id}/deliverables?path=output` | `{ path, entries: [{ name, path, kind, size, modified_ms }], truncated, scanned }`. `path` scopes the walk to a subdirectory. |
| `GET /api/projects/{id}/preview?path=output/report.docx` | `{ path, name, preview }` — see below. A file it cannot read is a `400` with the reason, a missing one a `404`. |
| `GET /api/projects/{id}/download?path=output/report.docx` | The raw bytes with their MIME type, `Content-Disposition: attachment`, `Cache-Control: no-store`. |

`preview` carries one payload per shape and omits the others:

```json
{
  "kind": "docx",
  "size": 18234,
  "passthrough": false,
  "truncated": false,
  "docx": {
    "title": "Q3 report",
    "author": "Ada",
    "paragraphs_total": 12,
    "words": 340,
    "paragraphs": ["Revenue grew 12%", "Costs fell 4%"],
    "truncated": false
  }
}
```

`passthrough: true` means the UI should fetch `/download` and render the bytes itself (PDF, images); `text` holds the plain-text kinds; `xlsx` and `pptx` hold `sheets` and `slides` respectively. Paths go through the same guard as the Files tab: `..` and absolute paths are rejected, and a symlink that resolves outside the project is refused.

## Boundaries

- **No visual fidelity.** The preview answers "what does this document say". Fonts, styles, colours, images, charts, merged cells, page breaks and number formats are not reproduced.
- **No rendering service.** Nothing is converted to PDF or to images, and no headless office suite is spawned. PDFs and images work only because the browser can render them.
- **OOXML only.** Legacy binary `.doc`/`.xls`/`.ppt` and `.docm`/`.pptm` macros are not previewed; re-export as `docx`/`xlsx`/`pptx`.
- **Truncation is silent on disk, visible in the UI.** The caps never modify the file — the download is always byte-identical to what the agent wrote.

## Implementation

- `src/office/mod.rs` — kind classification, limits, preview dispatch, plain-text reader.
- `src/office/zip.rs` — minimal ZIP reader (central directory, stored + deflate, no ZIP64, entry caps).
- `src/office/xml.rs` — minimal XML tokenizer (namespace prefixes stripped, entities decoded, malformed input ends the walk instead of panicking).
- `src/office/docx.rs`, `src/office/xlsx.rs`, `src/office/pptx.rs` — one extractor per format.
- `src/office/testing.rs` — package builders (`docx`, `xlsx`, `pptx`) so tests use real ZIP bytes.
- `src/web/handlers/delivery.rs` — the three REST handlers; the blocking read runs on `web::block`.
- `web/src/components/DeliverablePanel.tsx` — the Deliverables tab, preview and export.
