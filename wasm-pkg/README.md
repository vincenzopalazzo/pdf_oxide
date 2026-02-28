# pdf-oxide-wasm

High-performance PDF text extraction and manipulation via WebAssembly. Built on the [PDF Oxide](https://github.com/yfedoseev/pdf_oxide) Rust core.

## Quick Start

```javascript
const { WasmPdfDocument } = require("pdf-oxide-wasm");
const fs = require("fs");

const bytes = new Uint8Array(fs.readFileSync("document.pdf"));
const doc = new WasmPdfDocument(bytes);

console.log(`Pages: ${doc.pageCount()}`);
console.log(doc.extractText(0));

doc.free();
```

### ESM

```javascript
import { WasmPdfDocument } from "pdf-oxide-wasm";

const bytes = new Uint8Array(await fs.promises.readFile("document.pdf"));
const doc = new WasmPdfDocument(bytes);
const text = doc.extractText(0);
doc.free();
```

## Features

- Text extraction (plain text, Markdown, HTML)
- Character-level and span-level extraction with positions
- PDF creation from Markdown, HTML, text, and images
- Form field extraction and filling
- PDF editing (metadata, rotation, cropping, annotations)
- Encryption (AES-256)
- Search with regex support

## Documentation

Full API reference and examples: [Getting Started (WASM)](https://github.com/yfedoseev/pdf_oxide/blob/main/docs/getting-started-wasm.md)

## License

MIT OR Apache-2.0
