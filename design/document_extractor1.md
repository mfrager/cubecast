# Design: RDF representation of MinerU-extracted scholarly documents

## 1. Architectural objective

Build a canonical RDF document model that converts MinerU's structured PDF extraction output into a richly addressable semantic representation using three established vocabularies:

- DoCO — the structural composition of the document.
- BIBO — bibliographic identity and publication characteristics.
- Schema.org — interoperable descriptions of articles, people, organizations, images, tables, datasets, and other web-visible entities.

The design should preserve the original extracted content, its hierarchy, its physical position in the PDF, and its relationships to other content.

The resulting RDF graph should support:

1. Reconstructing the original document's logical structure.
2. Retrieving individual sections, paragraphs, sentences, figures, tables, and formulas.
3. Querying scholarly metadata and bibliographic relationships.
4. Linking content to its original PDF page and bounding box.
5. Supporting semantic search and knowledge extraction.
6. Preserving extraction provenance and confidence.
7. Adding scientific claims, methods, findings, and evidence later without redesigning the document model.

Important design decision: Do not attempt to make DoCO, BIBO, and Schema.org represent every detail themselves. Use their established classes and properties wherever appropriate, and introduce a small application namespace for extraction-specific information that these vocabularies do not adequately cover.

## 2. MinerU input contract

The design targets MinerU's structured intermediate representation, rather than treating its Markdown output as the primary input.

This is important because Markdown loses information that we need for a high-quality RDF representation.

The current MinerU documentation describes two relevant representations:

- `docvortex.middle` — ordered, postprocessed semantic blocks, with a schema identifier and version.
- `docvortex.model` — lower-level model analysis results.

It also provides structured content and Markdown as consumer-oriented outputs.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://opendatalab.github.io\&sz=32)

MinerU+1



### Recommended input

Use the following priority:

| Input                     | Purpose                                           | Priority             |
| ------------------------- | ------------------------------------------------- | -------------------- |
| `middle_json.json`        | Semantic blocks, hierarchy, spans, coordinates    | Primary              |
| `structured_content.json` | Simplified structured content                     | Fallback             |
| `markdown.md`             | Human-readable document reconstruction            | Secondary validation |
| Image assets              | Figures, tables, charts, extracted visual regions | Primary assets       |
| Raw model output          | Detection-level details and confidence            | Optional enrichment  |

Use the actual schema identifier and schema version to select the appropriate deserializer. MinerU's current and legacy formats are different; don't assume that a version number alone identifies the format.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://opendatalab.github.io\&sz=32)

MinerU+1



### Relevant MinerU data

The mapper should understand these structural elements:

| MinerU element     | RDF target                                                 |
| ------------------ | ---------------------------------------------------------- |
| Document metadata  | `bibo:AcademicArticle`, `schema:ScholarlyArticle`          |
| Page               | Application-specific `Page`                                |
| Title              | `doco:Title`, document title                               |
| Heading            | `doco:Section`                                             |
| Text block         | `doco:Paragraph`                                           |
| Text line          | Application-specific `Line`                                |
| Text span          | Application-specific `TextSpan`                            |
| Image              | `doco:Figure`, `schema:ImageObject`                        |
| Image caption      | `doco:Caption`                                             |
| Table              | `doco:Table`, `schema:Table`                               |
| Table caption      | `doco:Caption`                                             |
| Formula            | Application-specific `Formula`                             |
| List               | `doco:List`, list items                                    |
| Bibliography entry | `bibo:Document`, `bibo:AcademicArticle` where identifiable |
| Inline equation    | Application-specific `Formula`                             |
| Footnote           | `doco:Footnote`                                            |
| Page header/footer | Application-specific page auxiliary content                |

One distinction: MinerU's extraction of a reference entry does not necessarily mean it has resolved the referenced publication's identity. The graph should preserve both the raw reference and any subsequently resolved bibliographic resource.

### Important: preserve the original hierarchy

MinerU's intermediate representation can contain nested blocks, lines, and spans. Its output documentation also describes page indices, bounding boxes, and separate image/table/equation content.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://github.com\&sz=32)

GitHub+1



Do not flatten this into one string per page.

The intended structure is:

```
Document
  ├── Page 0
  │    ├── Title
  │    ├── Paragraph
  │    │    ├── Line
  │    │    │    ├── TextSpan
  │    │    │    └── TextSpan
  │    ├── Figure
  │    │    └── Caption
  │    └── Table
  ├── Page 1
  │    └── ...
  └── Logical Section Tree
       ├── Introduction
       │    ├── Paragraph
       │    └── Paragraph
       └── Methods
            └── ...
```

There are two distinct hierarchies here:

- Physical hierarchy: what appears on each PDF page.
- Logical hierarchy: what belongs to each section of the scholarly document.

They must coexist rather than being forced into a single parent-child tree.

## 3. Namespace and ontology design

I recommend the following namespace configuration.

| Prefix     | Namespace                                     | Responsibility                  |
| ---------- | --------------------------------------------- | ------------------------------- |
| `doco:`    | `http://purl.org/spar/doco/`                  | Document structure              |
| `bibo:`    | `http://purl.org/ontology/bibo/`              | Bibliographic resources         |
| `schema:`  | `https://schema.org/`                         | General web interoperability    |
| `dcterms:` | `http://purl.org/dc/terms/`                   | Descriptive metadata            |
| `foaf:`    | `http://xmlns.com/foaf/0.1/`                  | People and organizations        |
| `prov:`    | `http://www.w3.org/ns/prov#`                  | Extraction provenance           |
| `oa:`      | `http://www.w3.org/ns/oa#`                    | Document annotations            |
| `rdf:`     | `http://www.w3.org/1999/02/22-rdf-syntax-ns#` | RDF                             |
| `rdfs:`    | `http://www.w3.org/2000/01/rdf-schema#`       | RDF schema                      |
| `xsd:`     | `http://www.w3.org/2001/XMLSchema#`           | Datatypes                       |
| `ex:`      | `https://your-domain.example/ontology/`       | Application-specific extensions |

Use a real, stable domain under your control for the `ex:` namespace in production.

### Responsibility boundaries

DoCO

Use for the logical document components:

- `doco:Document`
- `doco:Section`
- `doco:Paragraph`
- `doco:Figure`
- `doco:Table`
- `doco:Caption`
- `doco:Footnote`
- `doco:List`
- `doco:ListItem`
- `doco:Abstract`
- `doco:BibliographicReference`

DoCO provides a structural vocabulary, not a complete representation of PDF geometry or text segmentation.

BIBO

Use for publication identity and bibliographic resources:

- `bibo:AcademicArticle`
- `bibo:Article`
- `bibo:Document`
- `bibo:Journal`
- `bibo:Book`
- `bibo:Chapter`
- `bibo:Thesis`
- `bibo:AcademicConference`

Use BIBO for the referenced publication, not for every piece of extracted text.

Schema.org

Use for broad interoperability:

- `schema:ScholarlyArticle`
- `schema:Person`
- `schema:Organization`
- `schema:ImageObject`
- `schema:Dataset`
- `schema:Table`
- `schema:DefinedTerm`
- `schema:CreativeWork`

For the paper itself, use both `bibo:AcademicArticle` and `schema:ScholarlyArticle` as types. This is a useful example of combining vocabularies without creating duplicate document resources.

One important implementation detail: use `schema:articleBody` for the article's complete body text when desired, but don't make it the only representation of the content. It is a text-valued property, whereas DoCO gives you independently addressable structural elements.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://schema.org\&sz=32)

Schema.org Type+1



## 4. Canonical graph structure

I recommend organizing the graph into eight resource families.

ScholarlyDocument

`bibo:AcademicArticle`

`schema:ScholarlyArticle`

Bibliography

`bibo:Document`

`schema:Person`

`schema:Organization`

Document structure

`doco:Section`

`doco:Paragraph`

`doco:Figure`

Physical layout

`ex:Page`

`ex:Line`

`ex:TextSpan`

Content objects

`doco:Table`

`ex:Formula`

`schema:ImageObject`

Extraction provenance

`prov:Entity`

`prov:Activity`

`prov:Agent`

### 4.1 Document resource

The root resource represents the intellectual scholarly publication.

```
ex:document/123
    a bibo:AcademicArticle,
      schema:ScholarlyArticle ;

    dcterms:title "Example Research Paper" ;

    schema:headline "Example Research Paper" ;

    schema:abstract "An example abstract." ;

    schema:author ex:person/456 ;

    dcterms:creator ex:person/456 ;

    ex:hasLogicalSection ex:section/123/1 ;

    ex:hasPhysicalPage ex:page/123/0 ;

    schema:datePublished "2025-01-01"^^xsd:date .
```

The same document URI must be used across all three vocabularies.

Do not create one BIBO document and a second Schema.org document that represent the same paper.

### 4.2 Authors and affiliations

Use Schema.org for people and organizations.

```
ex:person/456
    a schema:Person, foaf:Person ;

    schema:name "Jane Smith" ;

    schema:givenName "Jane" ;

    schema:familyName "Smith" ;

    schema:affiliation ex:organization/789 .

ex:organization/789
    a schema:Organization ;

    schema:name "Example University" .
```

Model author order explicitly.

`schema:author` alone does not preserve all the ordering and contribution semantics you may need.

I recommend an application-specific ordered author list:

```
ex:document/123
    ex:authorList ex:author-list/123 .

ex:author-list/123
    rdf:first ex:author-entry/1 ;
    rdf:rest rdf:nil .

ex:author-entry/1
    ex:person ex:person/456 ;
    ex:position 1 .
```

For multiple authors, use a proper RDF collection or an indexed list of author-entry resources.

This also gives you room to represent corresponding authors, equal contributions, and other roles later.

### 4.3 Logical sections

Each section is a DoCO section and can optionally be assigned a rhetorical classification.

```
ex:section/123/1
    a doco:Section ;

    dcterms:title "Introduction" ;

    ex:sectionLevel 1 ;

    ex:sectionNumber "1" ;

    ex:containsParagraph ex:paragraph/123/1 ;

    ex:sectionIndex 1 .
```

Use the section hierarchy to represent the logical structure, independently of page placement.

For example:

```
Paper
  └── Introduction
       ├── Background
       │    ├── Paragraph 1
       │    └── Paragraph 2
       └── Motivation
            └── Paragraph 3
```

I recommend `ex:containsSection` and `ex:containsParagraph` as application predicates, unless you choose to adopt the exact existing DoCO containment properties after checking the ontology's domain/range and intended semantics.

### 4.4 Paragraphs and sentences

Paragraphs should be first-class RDF resources.

```
ex:paragraph/123/1
    a doco:Paragraph ;

    rdf:value "Previous research has demonstrated..." ;

    ex:paragraphIndex 1 ;

    ex:belongsToSection ex:section/123/1 ;

    ex:hasSentence ex:sentence/123/1/1 .
```

For sentences:

```
ex:sentence/123/1/1
    a ex:Sentence ;

    rdf:value "Previous research has demonstrated..." ;

    ex:sentenceIndex 1 ;

    ex:belongsToParagraph ex:paragraph/123/1 .
```

I would not force sentences into `doco:Paragraph` or use `schema:Text` as their type. They are finer-grained content units that deserve a small extension.

This makes it possible to independently index and annotate each sentence without losing its original paragraph.

### 4.5 Figures and images

A figure should have one semantic resource, with its image representation and caption separately addressable.

```
ex:figure/123/1
    a doco:Figure ;

    ex:figureNumber "1" ;

    ex:hasCaption ex:caption/123/1 ;

    ex:hasImage ex:image/123/1 ;

    ex:appearsOnPage ex:page/123/2 .

ex:image/123/1
    a schema:ImageObject ;

    schema:contentUrl
        "assets/images/figure-1.png" ;

    schema:encodingFormat "image/png" .

ex:caption/123/1
    a doco:Caption ;

    ex:text "System architecture of the proposed framework." .
```

The image should be stored as an asset outside the RDF store, with a stable URI or content-addressed identifier.

Preserve:

- Original image bytes.
- Original image path from MinerU.
- Image dimensions.
- MIME type.
- Page number.
- Bounding box.
- Associated caption.
- Extraction engine and confidence.

Do not confuse a figure with its underlying image. A figure is a document component; an image is its visual representation.

### 4.6 Tables

Represent a table at two levels:

1. The complete table as a document component.
2. Its internal cells and rows as structured data.

```
ex:table/123/1
    a doco:Table, schema:Table ;

    ex:hasCaption ex:caption/table1 ;

    ex:hasTableStructure ex:table-structure/1 ;

    ex:appearsOnPage ex:page/123/4 .

ex:table-structure/1
    a ex:TableStructure ;

    ex:hasRow ex:table-row/1 ;
    ex:hasRow ex:table-row/2 .

ex:table-row/1
    a ex:TableRow ;

    ex:rowIndex 0 ;

    ex:hasCell ex:table-cell/1 ;
    ex:hasCell ex:table-cell/2 .

ex:table-cell/1
    a ex:TableCell ;

    ex:columnIndex 0 ;

    ex:text "Method" ;

    ex:isHeader true .

ex:table-cell/2
    a ex:TableCell ;

    ex:columnIndex 1 ;

    ex:text "Accuracy" ;

    ex:isHeader true .
```

Use explicit row and column indices, not RDF triple ordering.

Additional table properties should include:

- Row and column spans.
- Header associations.
- Cell type.
- Numeric values.
- Units.
- Footnotes.
- Merged cells.
- Original HTML or Markdown representation.
- Image fallback for tables that cannot be reconstructed reliably.

Important: preserve both the visual table and the extracted logical table. They are not necessarily equivalent.

### 4.7 Mathematical formulas

MinerU can extract inline and interline equations. Keep them as individual content resources.

I recommend:

```
ex:formula/123/1
    a ex:Formula ;

    ex:formulaType "interline" ;

    ex:latex "E = mc^2" ;

    ex:belongsToParagraph ex:paragraph/123/5 ;

    ex:appearsOnPage ex:page/123/5 .
```

Additional fields:

| Property                 | Meaning                            |
| ------------------------ | ---------------------------------- |
| `ex:latex`               | Extracted LaTeX                    |
| `ex:mathml`              | MathML if available                |
| `ex:formulaType`         | Inline or interline                |
| `ex:formulaNumber`       | Equation number                    |
| `ex:formulaImage`        | Original formula image             |
| `ex:sourceText`          | Original extracted text            |
| `ex:normalizationStatus` | Whether the formula was normalized |

Do not assume every equation can be converted perfectly into LaTeX. Preserve the original extraction and visual representation.

### 4.8 Lists, footnotes and code

Represent these as distinct structures rather than embedding everything into paragraph text.

| Component          | RDF representation            |
| ------------------ | ----------------------------- |
| List               | `doco:List`                   |
| List item          | `doco:ListItem`               |
| Footnote           | `doco:Footnote`               |
| Code block         | `ex:CodeBlock`                |
| Algorithm          | `ex:Algorithm`                |
| Bibliography entry | `doco:BibliographicReference` |

For ordered and unordered lists, preserve list type and item ordering.

For code blocks, retain the extracted source, programming language, and optional caption.

## 5. Physical document layout and PDF coordinates

This is one of the most important aspects of the design.

DoCO describes logical document composition, but MinerU also provides information about where content appears on individual PDF pages.

I recommend a separate physical layout model.

### Page representation

```
ex:page/123/4
    a ex:Page ;

    ex:pageIndex 4 ;

    ex:pageWidth 612.0 ;

    ex:pageHeight 792.0 ;

    ex:coordinateUnit "pt" ;

    ex:hasLayoutBlock ex:paragraph/123/12 .
```

Page indices should preserve MinerU's zero-based indexing.

For current MinerU outputs, page dimensions and geometry can be provided through its layout extension. The actual coordinate system must be read from the output format being consumed rather than assumed from a legacy parser.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://opendatalab.github.io\&sz=32)

MinerU+1



### Bounding boxes

Use a reusable geometry resource:

```
ex:geometry/123/12
    a ex:BoundingBox ;

    ex:x0 72.0 ;
    ex:y0 120.0 ;
    ex:x1 530.0 ;
    ex:y1 180.0 ;

    ex:coordinateUnit "pt" ;

    ex:coordinateOrigin "top-left" ;

    ex:page ex:page/123/4 .
```

Then attach it to the extracted content:

```
ex:paragraph/123/12
    ex:hasGeometry ex:geometry/123/12 .
```

This approach supports multiple geometry representations for the same content, including:

- Original PDF coordinates.
- Normalized coordinates.
- Rotated page coordinates.
- OCR-derived coordinates.
- Alternative extraction-engine coordinates.

Do not overwrite original geometry when converting between coordinate systems.

### Use Web Annotation for text anchoring

For passages that need to be independently addressable, Web Annotation is a useful complement.

For example, a claim extracted from a paragraph can refer to a precise text selection:

```
ex:annotation/123/1
    a oa:Annotation ;

    oa:hasTarget [
        a oa:SpecificResource ;

        oa:hasSource ex:document/123 ;

        oa:hasSelector [
            a oa:TextQuoteSelector ;

            oa:exact "The proposed method improves performance." ;

            oa:prefix "Our experimental results show that " ;

            oa:suffix " across all test datasets."
        ]
    ] .
```

This provides a portable way to identify content fragments without relying exclusively on internal paragraph identifiers. Web Annotation supports text quote, text position, and other selector types.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C+1



I would use both:

- Internal RDF identifiers for structural relationships.
- Web Annotation selectors for robust source grounding.

## 6. Content ordering and reconstruction

RDF graphs are unordered sets of triples. Consequently, document order must be explicitly represented.

I recommend integer ordering properties.

```
ex:section/123/1
    ex:sectionIndex 1 .

ex:paragraph/123/1
    ex:paragraphIndex 1 .

ex:paragraph/123/2
    ex:paragraphIndex 2 .
```

For lists and ordered structures, use RDF collections where their semantics genuinely fit, or indexed membership resources for more flexible querying.

### Two separate orderings

Logical ordering

The position of an element within the scholarly document.

```
Introduction
  Paragraph 1
  Paragraph 2
Methods
  Paragraph 3
Results
  Paragraph 4
```

Physical ordering

The position of an element within the original PDF.

```
Page 1
  Block 1
  Block 2
  Block 3

Page 2
  Block 1
  Block 2
```

A paragraph may span multiple pages, and a page may contain paragraphs belonging to multiple logical sections.

Therefore, avoid treating `ex:appearsOnPage` as equivalent to `ex:belongsToSection`.

### Reading order

Store a reading-order index for each physical block.

```
ex:paragraph/123/12
    ex:readingOrderIndex 17 ;
    ex:appearsOnPage ex:page/123/4 .
```

This is especially important for two-column academic papers, sidebars, footnotes, and floating figures.

## 7. Bibliography and citation representation

A scholarly document's bibliography should be independently modeled.

There are three distinct things:

1. The bibliography entry as printed in the current paper.
2. The actual referenced publication.
3. The occurrence of a citation in the body text.

These should not be collapsed into one RDF resource.

### Bibliography entry

```
ex:reference/123/1
    a doco:BibliographicReference ;

    ex:rawReferenceText
      "Smith, J. (2024). Example Research. Journal of Science." ;

    ex:referencesPublication ex:publication/456 ;

    ex:referenceIndex 1 .
```

### Referenced publication

```
ex:publication/456
    a bibo:AcademicArticle,
      schema:ScholarlyArticle ;

    dcterms:title "Example Research" ;

    schema:author ex:person/456 ;

    bibo:doi "10.1234/example.2024.1" .
```

The DOI example illustrates the desired data, but the exact BIBO property and identifier representation should be validated against the ontology version you adopt. I would also preserve DOI as a normalized identifier and generate its canonical resolver URI separately.

### In-text citation occurrence

```
ex:citation/123/1
    a ex:CitationOccurrence ;

    ex:citesReference ex:reference/123/1 ;

    ex:appearsInParagraph ex:paragraph/123/5 ;

    ex:rawCitationText "[1]" .
```

This distinction makes it possible to represent multiple citations to the same publication.

For example:

- One publication may have one bibliography entry.
- That entry may be cited 15 times.
- Each citation may appear in a different paragraph.

Later, CiTO can add relationships such as agreement, disagreement, extension, and methodological reuse without changing the document model.

## 8. Extraction provenance and confidence

The RDF model should preserve which extraction engine produced each resource.

Use PROV-O rather than inventing a complete provenance vocabulary.

### Extraction activity

```
ex:extraction/123
    a prov:Activity ;

    prov:used ex:source-pdf/123 ;

    prov:wasAssociatedWith ex:mineru-agent ;

    prov:generated ex:document/123 ;

    ex:extractorVersion "4.0.0" ;

    ex:extractorBackend "pipeline" ;

    ex:extractorTier "default" ;

    ex:inputSchema "docvortex.middle" ;

    ex:inputSchemaVersion "2.0" .

ex:mineru-agent
    a prov:SoftwareAgent ;

    schema:name "MinerU" .
```

The version above is illustrative; use the actual producer metadata.

### Per-element provenance

A document may be processed by multiple engines. A paragraph might originate from MinerU, while its caption was subsequently corrected using another model.

I recommend recording provenance at the individual resource level:

```
ex:paragraph/123/1
    prov:wasGeneratedBy ex:extraction/123 .

ex:caption/123/1
    prov:wasGeneratedBy ex:caption-extraction/123 .
```

### Confidence

MinerU may expose confidence information in certain output modes. Preserve it without treating every score as having the same statistical meaning.

```
ex:paragraph/123/1
    ex:extractionConfidence 0.97 ;

    ex:confidenceSource "mineru-model-score" .
```

Use an explicit confidence-source identifier and avoid comparing confidence values across different models without calibration.

I would also preserve:

- Raw extraction block.
- Original block index.
- Original page index.
- Original bounding box.
- Extraction timestamp.
- MinerU version.
- Processing backend.
- Normalization operations.
- Validation status.

PROV-O provides the standard framework for connecting these resources and activities.&#x20;

[image](https://www.google.com/s2/favicons?domain=https://www.w3.org\&sz=32)

W3C+1



## 9. Stable identifier design

Use deterministic identifiers for extracted resources.

This is important because the same document may be reprocessed using a newer MinerU version.

Suggested URI patterns:

| Resource            | URI pattern                                    |
| ------------------- | ---------------------------------------------- |
| Document            | `/document/{document_id}`                      |
| Page                | `/document/{id}/page/{page}`                   |
| Section             | `/document/{id}/section/{path}`                |
| Paragraph           | `/document/{id}/paragraph/{index}`             |
| Sentence            | `/document/{id}/sentence/{paragraph}/{index}`  |
| Figure              | `/document/{id}/figure/{index}`                |
| Table               | `/document/{id}/table/{index}`                 |
| Formula             | `/document/{id}/formula/{index}`               |
| Reference           | `/document/{id}/reference/{index}`             |
| Citation occurrence | `/document/{id}/citation/{index}`              |
| Geometry            | `/document/{id}/geometry/{element}/{revision}` |
| Extraction run      | `/extraction/{run_id}`                         |

I recommend separating document identity from extraction identity.

For example:

```
document/abc123
    ├── extraction/run001
    │    ├── paragraph/1
    │    └── figure/1
    │
    └── extraction/run002
         ├── paragraph/1
         └── figure/1
```

This allows re-extraction without destroying historical results.

For content-addressed identity, use a hash of the original PDF as a document identity candidate, but don't assume that two identical PDFs necessarily represent two identical scholarly publications. Maintain separate publication identity and file identity.

## 10. Complete mapping specification

This is the proposed MinerU-to-RDF mapping contract.

| MinerU data         | RDF type                                          | Key properties                      |
| ------------------- | ------------------------------------------------- | ----------------------------------- |
| Document            | `bibo:AcademicArticle`, `schema:ScholarlyArticle` | `dcterms:title`, `schema:abstract`  |
| Authors             | `schema:Person`                                   | `schema:name`, `schema:affiliation` |
| Affiliations        | `schema:Organization`                             | `schema:name`                       |
| Page                | `ex:Page`                                         | `ex:pageIndex`, dimensions          |
| Title block         | Document metadata / title component               | `schema:headline`                   |
| Heading             | `doco:Section`                                    | `dcterms:title`, section level      |
| Text block          | `doco:Paragraph`                                  | `ex:text`                           |
| Line                | `ex:Line`                                         | `ex:text`, geometry                 |
| Span                | `ex:TextSpan`                                     | `ex:text`, style, geometry          |
| Figure              | `doco:Figure`                                     | Caption, image                      |
| Image               | `schema:ImageObject`                              | `schema:contentUrl`, dimensions     |
| Caption             | `doco:Caption`                                    | `ex:text`                           |
| Table               | `doco:Table`, `schema:Table`                      | Caption, table structure            |
| Table row           | `ex:TableRow`                                     | Row index                           |
| Table cell          | `ex:TableCell`                                    | Cell text, row/column index         |
| Equation            | `ex:Formula`                                      | LaTeX, MathML                       |
| List                | `doco:List`                                       | List type, order                    |
| List item           | `doco:ListItem`                                   | Item order                          |
| Footnote            | `doco:Footnote`                                   | Text, source position               |
| Bibliography entry  | `doco:BibliographicReference`                     | Raw text, resolved work             |
| Citation occurrence | `ex:CitationOccurrence`                           | Reference, paragraph                |
| Extraction          | `prov:Activity`                                   | Engine, version, configuration      |
| Geometry            | `ex:BoundingBox`                                  | Coordinates, page                   |

A few mappings require semantic judgment rather than direct conversion:

- A MinerU title block is not necessarily the document's actual title. It might be a section heading.
- A MinerU text block is not necessarily a paragraph. It might be a reference, footnote, page header, or caption.
- A detected table is not necessarily a correctly reconstructed table.
- A detected image is not necessarily a scholarly figure.

The mapper should use the block's context and classification, not just its raw type.

## 11. RDF named graph organization

For Oxigraph, I would use named graphs to separate document content, source layout, metadata, and provenance.

Oxigraph dataset

`graph/document/{id}`

Canonical document structure, paragraphs, sections, figures, tables and formulas.

`graph/bibliography/{id}`

Publication metadata, authors, affiliations, bibliography and citations.

`graph/layout/{id}`

PDF pages, reading order, coordinates, lines, spans and image locations.

`graph/provenance/{id}`

MinerU runs, versions, confidence, derivations and validation history.

`graph/ontology`

Vocabulary declarations, ontology imports and application schemas.

Use a separate graph for each extraction revision if you want to retain multiple competing extraction results.

For example:

```
graph/document/abc123/run001
graph/document/abc123/run002
graph/layout/abc123/run001
graph/provenance/abc123
```

A document-level catalog graph can point to the currently accepted revision.

### Why this organization?

It provides several useful properties:

- Bibliographic queries don't need to traverse page geometry.
- Document structure can be replaced without rewriting publication metadata.
- Multiple extraction versions can coexist.
- Provenance can be audited independently.
- You can load or export only selected graph partitions.

One caveat: named graphs are organizational boundaries, not automatic security or inference boundaries. Your application must enforce those separately.

## 12. SHACL validation

I would make SHACL validation a required stage after RDF generation.

Some important constraints:

| Shape      | Validation                                                       |
| ---------- | ---------------------------------------------------------------- |
| Document   | Must have one document type and stable identifier                |
| Section    | Must have a title or identifiable heading                        |
| Paragraph  | Must have text                                                   |
| Figure     | Must have an image or identifiable visual representation         |
| Table      | Must have a table representation                                 |
| Page       | Must have a non-negative index                                   |
| Geometry   | Must have valid coordinates and a page                           |
| Reference  | Must retain raw reference text                                   |
| Citation   | Must point to a valid reference or unresolved reference resource |
| Extraction | Must identify its producing engine and input                     |

Example SHACL:

```
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <https://example.org/ontology/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .

ex:ParagraphShape
    a sh:NodeShape ;

    sh:targetClass ex:Paragraph ;

    sh:property [
        sh:path ex:text ;
        sh:datatype xsd:string ;
        sh:minCount 1 ;
    ] ;

    sh:property [
        sh:path ex:paragraphIndex ;
        sh:datatype xsd:integer ;
        sh:minCount 1 ;
    ] .

ex:PageShape
    a sh:NodeShape ;

    sh:targetClass ex:Page ;

    sh:property [
        sh:path ex:pageIndex ;
        sh:datatype xsd:integer ;
        sh:minCount 1 ;
        sh:minInclusive 0 ;
    ] .
```

For production, I would also validate cross-resource consistency:

- Section parent-child relationships.
- Page references.
- Figure-caption relationships.
- Citation-reference relationships.
- Table row and column ordering.
- Geometry bounds against page dimensions.
- Duplicate identifiers.
- Broken asset references.

Validation should distinguish hard errors from extraction warnings. For example, a figure with no caption may be valid, even if it is worth flagging.

## 13. Rust implementation architecture

I would separate the implementation into five components.

```
mineru-rdf/
├── input/
│   ├── middle_json.rs
│   ├── structured_content.rs
│   ├── legacy_middle.rs
│   └── assets.rs
│
├── model/
│   ├── document.rs
│   ├── metadata.rs
│   ├── section.rs
│   ├── paragraph.rs
│   ├── figure.rs
│   ├── table.rs
│   ├── formula.rs
│   ├── bibliography.rs
│   ├── geometry.rs
│   └── provenance.rs
│
├── mapping/
│   ├── document.rs
│   ├── bibliographic.rs
│   ├── structure.rs
│   ├── visual.rs
│   ├── layout.rs
│   └── provenance.rs
│
├── rdf/
│   ├── namespaces.rs
│   ├── identifiers.rs
│   ├── graph.rs
│   ├── triples.rs
│   ├── serializer.rs
│   └── validation.rs
│
└── ontology/
    ├── document.ttl
    ├── shapes.ttl
    └── namespaces.ttl
```

### Rust domain types

I would create a normalized intermediate representation that is independent of MinerU and RDF.

For example:

```
pub struct ExtractedDocument {
    pub id: DocumentId,
    pub metadata: DocumentMetadata,
    pub pages: Vec<Page>,
    pub sections: Vec<Section>,
    pub references: Vec<Reference>,
    pub citations: Vec<Citation>,
    pub provenance: ExtractionProvenance,
}

pub struct Section {
    pub id: ElementId,
    pub title: Option<String>,
    pub level: u32,
    pub order: u32,
    pub paragraphs: Vec<ElementId>,
    pub children: Vec<ElementId>,
}

pub struct Paragraph {
    pub id: ElementId,
    pub text: String,
    pub order: u32,
    pub sentences: Vec<Sentence>,
    pub geometry: Vec<GeometryRef>,
}

pub struct Sentence {
    pub id: ElementId,
    pub text: String,
    pub order: u32,
}

pub struct Figure {
    pub id: ElementId,
    pub caption: Option<String>,
    pub asset: Option<AssetRef>,
    pub geometry: Vec<GeometryRef>,
}

pub struct Table {
    pub id: ElementId,
    pub caption: Option<String>,
    pub rows: Vec<TableRow>,
    pub geometry: Vec<GeometryRef>,
}

pub struct Formula {
    pub id: ElementId,
    pub latex: Option<String>,
    pub mathml: Option<String>,
    pub formula_type: FormulaType,
    pub geometry: Vec<GeometryRef>,
}
```

I would also define a common trait for RDF serialization:

```
pub trait RdfResource {
    fn subject(&self) -> NamedNode;

    fn to_quads(
        &self,
        context: &RdfContext,
    ) -> Result<Vec<Quad>>;
}
```

Each resource type can then produce RDF independently.

The mapper should be deterministic: processing the same MinerU input with the same configuration should produce the same resource identifiers and RDF statements.

### Oxigraph integration

Use the Rust `oxigraph` crate.

The important operations are:

- Create named nodes.
- Create literals.
- Create quads.
- Insert quads into a Store.
- Execute SPARQL queries.
- Serialize RDF datasets.
- Load ontology definitions and SHACL shapes.

I would keep RDF generation separate from storage so the same normalized document can be exported to Turtle, TriG, JSON-LD, or inserted directly into Oxigraph.

## 14. Example SPARQL queries

These illustrate the practical benefit of the model.

### Find all documents containing a particular concept

```
SELECT DISTINCT ?document ?title
WHERE {
    ?document a bibo:AcademicArticle ;
              dcterms:title ?title ;
              ex:hasLogicalSection ?section .

    ?section ex:containsParagraph ?paragraph .

    ?paragraph ex:mentionsConcept ex:concept/distributed-computing .
}
```

### Retrieve a document's complete logical structure

```
SELECT ?section ?sectionTitle ?paragraph ?text
WHERE {
    ex:document/123 ex:hasLogicalSection ?section .

    ?section dcterms:title ?sectionTitle ;
             ex:containsParagraph ?paragraph .

    ?paragraph ex:text ?text .
}
ORDER BY ?sectionIndex ?paragraphIndex
```

### Find all figures and their captions

```
SELECT ?figure ?caption ?image
WHERE {
    ex:document/123 ex:hasLogicalSection ?section .

    ?section ex:containsFigure ?figure .

    OPTIONAL {
        ?figure ex:hasCaption ?captionResource .
        ?captionResource ex:text ?caption .
    }

    OPTIONAL {
        ?figure ex:hasImage ?image .
    }
}
```

### Retrieve all citations in a particular paragraph

```
SELECT ?citation ?reference ?publication
WHERE {
    ?citation a ex:CitationOccurrence ;
              ex:appearsInParagraph ex:paragraph/123/5 ;
              ex:citesReference ?reference .

    OPTIONAL {
        ?reference ex:referencesPublication ?publication .
    }
}
```

### Find every occurrence of a publication across the corpus

```
SELECT ?document ?paragraph
WHERE {
    ?reference ex:referencesPublication ex:publication/456 .

    ?citation ex:citesReference ?reference ;
              ex:appearsInParagraph ?paragraph .

    ?document ex:hasLogicalSection ?section .
    ?section ex:containsParagraph ?paragraph .
}
```

For this final query, I would add a direct paragraph-to-document relationship or a section-to-document relationship to avoid needing to infer document ownership through the complete hierarchy.

## 15. Implementation phases

I would build this incrementally rather than trying to implement the entire ontology immediately.

| Phase | Deliverable              | Main work                                                      |
| ----- | ------------------------ | -------------------------------------------------------------- |
| 1     | MinerU ingestion         | Parse current and legacy JSON, assets, metadata, page geometry |
| 2     | Canonical document model | Sections, paragraphs, figures, tables, formulas, references    |
| 3     | RDF mapping              | DoCO, BIBO, Schema.org, application predicates                 |
| 4     | Layout and provenance    | Coordinates, Web Annotation, PROV-O, extraction revisions      |
| 5     | Validation and querying  | SHACL, SPARQL, Oxigraph, export and integration tests          |

I would make Phase 2 particularly rigorous. If the intermediate document model is correct, supporting other extraction engines later becomes much easier.

## 16. Final design decisions

Here is the set of decisions I would adopt as the initial specification.

| Design decision              | Choice                             |
| ---------------------------- | ---------------------------------- |
| Primary extraction source    | MinerU `docvortex.middle`          |
| Fallback source              | Structured Content                 |
| Primary document vocabulary  | DoCO                               |
| Bibliographic vocabulary     | BIBO                               |
| General interoperability     | Schema.org                         |
| Common metadata              | Dublin Core Terms                  |
| PDF geometry                 | Application-specific vocabulary    |
| Text anchoring               | W3C Web Annotation                 |
| Extraction provenance        | PROV-O                             |
| Canonical intermediate model | Typed Rust structures              |
| RDF database                 | Oxigraph                           |
| RDF validation               | SHACL                              |
| RDF export                   | Turtle, TriG, JSON-LD              |
| Resource identification      | Stable deterministic IRIs          |
| Multiple extraction runs     | Separate named graphs              |
| Original assets              | External content-addressed storage |

### One extension I would deliberately defer

I would not initially introduce a complete scientific knowledge ontology for claims, evidence, findings, and experimental methods.

Instead, leave extension points for:

```
ScientificKnowledge
├── Claim
├── Hypothesis
├── Definition
├── Method
├── Experiment
├── Observation
├── Measurement
├── Finding
└── Evidence
```

These are conceptually different from document components.

A paragraph is something the paper contains. A claim is something the paper asserts. An observation is something an experiment reports. One paragraph might contain multiple claims, and one claim might be supported by several pieces of evidence.

Keeping these distinctions allows the document model to remain faithful to MinerU's extraction while enabling a much richer knowledge representation later.

The central architectural principle is to preserve three independent dimensions: scholarly document structure (DoCO), bibliographic identity (BIBO/Schema.org), and physical source layout (your own geometry model). Everything else—including scientific knowledge extraction—can build on those foundations without losing the connection to the original PDF.
