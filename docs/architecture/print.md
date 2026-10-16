---
title: "Print templates and PDF"
description: "The only PDF the kernel writes is a one-page table (facets/report/src/export.rs, hand-written Helvetica), and facets/document is an empty stub."
section: architecture
status: proposal
order: 350
---
# Print templates and PDF (proposal)

**Status: proposal, nothing built.** Choices are marked *Decide* in the last section.

The only PDF the kernel writes is a one-page table (`facets/report/src/export.rs`, hand-written Helvetica), and
`facets/document` is an empty stub. Invoices, delivery notes, payslips, contracts and letters are printed in every
business. This page designs templates as data, a renderer that runs outside the server process, and archiving that keeps
what was actually sent.

## A template is data

`print/<name>.xml` in the plugin, bound to a model, using the same expression syntax as message templates in
[Automation](automation.md#templates):

```xml
<print name="invoice" model="invoice" paper="A4" margin="18mm" title="Invoice {{record.number}}">
  <header><image src="{{company.logo}}" height="14mm"/> <text align="right">{{company.name}}</text></header>
  <section>
    <text weight="bold">{{record.customer.name}}</text>
    <address value="{{record.customer.address}}"/>
  </section>
  <table rows="record.lines" repeat-header="true">
    <col label="Item"  value="{{row.product.name}}"/>
    <col label="Qty"   value="{{row.qty|number}}" align="right"/>
    <col label="Total" value="{{row.total|money(record.currency)}}" align="right"/>
  </table>
  <totals>
    <row label="Total" value="{{record.total|money(record.currency)}}"/>
  </totals>
  <footer>Page {{page}} of {{pages}} · {{company.bank}}</footer>
</print>
```

- A small element set (text, table, image, address, barcode/QR, page break, columns), not free HTML, so the output is
  predictable and the template can be checked statically ([Verification](verification.md)).
- Filters (`money`, `date`, `number`, `percent`, `join`) format per the document's **language and currency**, not the
  server's ([Localization](localization.md)); a template may have per-language variants.
- Data is read **as the person printing**, through record and field rules: a field they cannot see prints blank or the
  template's `hidden` text, it is never fetched around the rules.
- Organizations can override a template (layout, logo, wording) through [Customization](customization.md); a plugin
  upgrade does not overwrite their version.

## The renderer

PDF generation is CPU-heavy and parses untrusted content, so it runs **in a separate process**, like OCR engines
(`bridges/document_parsing`): a bridge under `bridges/document_rendering/` accepts the laid-out document and returns
bytes. The kernel does layout from the XML to a small intermediate form; the engine turns that into PDF (PDF/A-3 when
asked, so an XML invoice can be embedded for e-invoicing). Images are fetched from storage by the kernel, never by the engine.
Fonts are bundled, plus organization fonts from storage. Limits: pages, time, memory, as for OCR.

## Archiving what was sent

`print::render` can be told `archive: true`: the PDF is stored as a media file linked to the record, with its template
version and the hash of the data used, and shown in the record's attachments. Posting an invoice by email sends the archived
file, not a re-render, so what the customer got can always be shown again byte for byte. Re-printing later makes a new
version and keeps the old one.

## Where it is used

- A **Print** button on form pages (`<view type="form" print="invoice"/>`) and a list action for batches (one PDF per
  record or a merged file).
- Attachments of [automation](automation.md) emails and of `communication::send`.
- [Federation](federation.md) envelopes, which carry the archived PDF and the structured data together.

## Commands

| Command | Capability | Does |
|---|---|---|
| `print::render { template, record, lang?, archive? }` | `print::render` | returns `{ media_key, pages, hash }` |
| `print::batch { template, filter }` | `print::render` | job with a callback; one file or a zip |
| `print::templates { model }` | `print::read` | what exists and the organization's overrides |

## Settings and tables

Settings: `print.engine` (bridge), `print.max_pages`, `print.timeout_secs`, `print.default_paper`,
`print.archive_default`. Tables: `print_archive` (record, template, template version, data hash, media key, who, when).

## Not in scope

A drag-and-drop template designer (the XML is the model; an editor page can come later), word-processor round trips,
mail-merge into user-uploaded `.docx` (a possible bridge).

## Build order

1. Template format, layout, locale filters; render a table-and-text document.
2. Engine bridge in a separate process; archiving.
3. Print buttons and batches.
4. QR/barcode, PDF/A-3 with embedded data.
5. Organization overrides.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Free HTML templates or a fixed element set? | **Fixed element set**: predictable, checkable. |
| 2 | Where does rendering run? | **A separate process via a bridge**, not in the kernel. |
| 3 | Does an emailed document get re-rendered? | **No**, the archived file is sent. |
| 4 | Whose rights apply to the data? | **The person printing**. |
