Yes. MinerU can produce a single continuous text representation of a multi-page document, excluding pictures and other visual blocks. You don't need to concatenate page text yourself if you use its text-only Markdown renderer.

There are two approaches.

## Option 1: Text-only Markdown (recommended)

MinerU supports `MakeMode.NLP_MD`, which produces text-only Markdown and skips visual blocks.

This is the closest match to what you're describing:

- All pages combined into one output string.
- Text retained in reading order.
- Headings and paragraphs retained.
- Images skipped.
- Tables and other visual blocks skipped.
- Mathematical content can be retained as text/LaTeX where extracted.

The rendering modes are documented in the MinerU output implementation.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://app.dosu.dev\&sz=32)

Dosu+1



Conceptually:

```
from mineru.render import render, RenderFormattext = render(    parse_result,    RenderFormat.MARKDOWN,    # Use the NLP_MD rendering mode)
```

The exact public API for selecting `NLP_MD` depends on the MinerU version; it is a rendering mode, not necessarily a `render()` argument.

## Option 2: Concatenate extracted text blocks yourself

MinerU also offers `content_list.json`, a flattened list of document content blocks in reading order.

You can filter out everything except text:

```
import jsondata = json.load(open("content_list.json"))text = "\n\n".join(    item["text"]    for item in data    if item["type"] == "text")
```

This example targets the legacy Content List V1 format. Newer output formats have different field names and nesting.

MinerU's Content List V2 preserves page grouping, whereas V1 provides a flat list.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub+1



## Which should you use?

| Requirement                           | Best choice            |
| ------------------------------------- | ---------------------- |
| One continuous text string            | NLP Markdown           |
| Only textual blocks                   | Content List filtering |
| Preserve headings and hierarchy       | Structured Content     |
| Preserve coordinates and page numbers | Middle JSON            |
| Build your RDF document graph         | Middle JSON            |

My recommendation for your RDF pipeline: retain `middle_json` as the authoritative extraction result, but additionally generate a single text-only Markdown string.

That gives you both:

- A clean, continuous text document for LLM processing, embedding, and full-text search.
- The complete structured extraction for reconstructing DoCO sections, paragraphs, figures, tables, and page locations.

You can also generate a plain-text version by stripping Markdown formatting from the NLP Markdown output.
