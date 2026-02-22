# PDF Oxide - Python Bindings

High-performance PDF parsing for Python with PDF specification compliance.

## Features

- **PDF Spec Compliance**: ISO 32000-1:2008 sections 9, 14.7-14.8
- **Intelligent Text Extraction**: Automatic reading order detection
- **Multi-Column Support**: 4 pluggable layout strategies
- **Font Recovery**: 70-80% character recovery with advanced font support
- **Complex Scripts**: RTL (Arabic/Hebrew), CJK (Japanese/Korean/Chinese), Devanagari, Thai
- **OCR Support**: Optional DBNet++/SVTR for scanned PDFs
- **Format Conversion**: Markdown, HTML, PlainText
- **Performance**: 1.0ms mean — 5× faster than PyMuPDF, 14× faster than pypdf, 100% pass rate on 3,830 PDFs

## Quick Start

```python
from pdf_oxide import PdfDocument

# Open a PDF
doc = PdfDocument("document.pdf")

# Extract as plain text (with automatic reading order)
text = doc.to_plain_text(0)
print(text)

# Convert to Markdown
markdown = doc.to_markdown(0, detect_headings=True)
with open("output.md", "w") as f:
    f.write(markdown)

# Convert to HTML
html = doc.to_html(0, preserve_layout=False)
with open("output.html", "w") as f:
    f.write(html)
```

## Installation

```bash
pip install pdf_oxide
```

## Development
```bash
# Install uv
# refer to https://docs.astral.sh/uv/getting-started/installation/#standalone-installer

# Install necessary tools for python dev
uv tool install maturin
uv tool install pdm
uv tool install ruff
uv tool install ty

# Install python dependencies
uv sync --group test

# If you need to run scripts, please add the responsive group
# e.g., if you need to run "benchark_all_libraries.py" script, you should run
uv sync --group benchmark
# All the groups could be found in [dependency-groups] in "pyproject.toml"

# If you just need production code, please run
uv sync

# Build python bindings
maturin develop --uv

# format code (would format both python and rust code)
pdm fmt

# lint code (would lint both python and rust code)
pdm lint
```


## API Documentation

See the main README for full API documentation and examples.
