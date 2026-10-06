# Deduplication: finding records that are the same thing (proposal)

**Status: proposal, nothing built.** Every choice below is marked *Decide* in the last section so we can
keep, change or drop it before any code is written.

A plugin declares that some of its models can have duplicates (a party, an employee, a lead) and which
fields say so. The kernel finds the groups of records that probably describe the same real-world thing,
by exact match, fuzzy text, phonetic codes or, when the organization has configured it, OpenSearch.
It must be fast on millions of rows and cheap enough to run on every create.

## Why a facet, not a plugin

A plugin call is 50 million instructions of fuel, 10 s, 16 MB and a 64 KiB payload; reading about 700
records is already near the limit (see [Scheduler](scheduler.md#running-one-function-over-many-calls-at-once-fan-out)).
Matching is CPU work over many rows, so a plugin would be orders of magnitude too slow and would need
`aether_sdk::parallel` chunks that cannot even see each other's data. A native crate gets real threads,
streaming reads, an HTTP client with pooling and a warm in-memory index.

What stays in plugins is the *knowledge*: which models, which fields, how much each field counts. A plugin
ships a rule file and calls kernel commands, exactly as it does for `db::*` and `communication::send`.

## Where each piece lives

```
plugins/…/dedup/<model>.json        rule files shipped by a plugin (data, like rules/ and models/)
        │ loaded and checked with the plugin
        ▼
facets/core  host/dedup.rs          kernel commands, capability check, model grants, record rules,
        │                           tables, scheduler job kind `dedup`
        ▼
facets/dedup  (aether_dedup)        the engine: pure Rust, no database, no kernel types
        │   normalize · phonetic · block · score · cluster
        ▼ trait CandidateSource           (implemented in facets/core, calls the bridge like any client)
bridges/search/opensearch           a general OpenSearch bridge: knows indexes and queries, nothing about
                                    dedup; plugins call it too (see below)
```

**The engine crate does not depend on `aether_core`.** It takes records as plain values and answers with
pairs and groups. That keeps the dependency pointing one way (`core` calls `dedup`, never the reverse),
lets us benchmark and fuzz it without a database, and keeps `facets/core` from growing again.
`facets/core` owns everything that needs the kernel: reading rows through the data model, access rules,
tables, jobs.

OpenSearch is a **general-purpose bridge**, not part of the facet: bridges are already where an external
service's settings and secrets live (database-held, organization override, secrets encrypted; see
[Bridges](../bridges.md)). The bridge knows about indexes, documents and queries and **nothing about
deduplication**; dedup is just one client of it, and any plugin may be another. The engine crate sees only
the `CandidateSource` trait, which `facets/core` implements by calling the bridge, so a deployment without
OpenSearch loses nothing.

## A rule file

One file per model, in the plugin's `dedup/` folder, validated at load against the model's fields like
`rules/<model>.json`.

```json
{
  "model": "party",
  "key": "party_name_match",
  "preset": "people",
  "locale_field": "country",
  "fields": [
    { "field": "name",  "normalize": ["lower", "fold_accents", "collapse_space", "strip_titles"],
      "match": "name", "weight": 4.0, "phonetic": ["double_metaphone", "cologne"] },
    { "field": "email", "normalize": ["lower", "trim"],            "match": "exact",   "weight": 6.0, "unique_evidence": true },
    { "field": "phone", "normalize": ["digits", "e164"],           "match": "exact",   "weight": 5.0, "unique_evidence": true },
    { "field": "birth_date",                                         "match": "date",    "weight": 3.0, "tolerance_days": 0 },
    { "field": "address", "match": "address", "weight": 2.0 },
    { "field": "city",  "normalize": ["lower", "fold_accents"],    "match": "edit",    "weight": 1.0 }
  ],
  "block": [
    ["email"], ["phone"],
    ["name:phonetic", "birth_date"],
    ["name:token_sorted:prefix4"],
    ["address:ngram"]
  ],
  "thresholds": { "match": 0.90, "review": 0.75 },
  "scope": { "same": ["company"] },
  "ignore": { "status": "archived" }
}
```

| Part | Meaning |
|---|---|
| `fields` | what is compared, how it is cleaned first, how two values are scored (0 to 1) and how much it counts |
| `match` | one scoring method from the [catalog](#matching-methods-the-catalog) (`exact`, `edit`, `jaro_winkler`, `token_set`, `ngram`, `name`, `address`, `date`, `number`, `geo`, `list`, `embedding`, …) |
| `phonetic` | extra codes derived from the field, used for blocking and as a scoring bonus |
| `unique_evidence` | an exact hit on this field alone is strong enough to propose a pair (email, tax id) |
| `block` | the **blocking keys**: a list of key recipes; two records are compared only if they share at least one key |
| `thresholds` | at or above `match` the pair is a likely duplicate; between `review` and `match` it goes to a person |
| `scope.same` | only compare records that agree on these fields (same company, same organization unit) |
| `ignore` | records to leave out, as a filter (see [Queries](queries.md)) |

A rule is data: no code from the plugin runs during matching, so matching never touches the plugin
sandbox limits. A plugin can ship several rules for one model (`key` tells them apart).

## Matching methods: the catalog

Phonetic, fuzzy and OpenSearch are three entries in a longer list. The engine treats every method as a
plug-in implementing one of three small traits, so adding a method never touches the rest of the engine:

| Trait | Job | Output |
|---|---|---|
| `Normalizer` | clean a value before anything else | a canonical value |
| `Blocker` | decide **which** records are worth comparing | keys, or candidate ids |
| `Scorer` | decide **how similar** two candidates are | 0 to 1, plus an explanation |

All methods are built; **which ones run is the user's choice**, per rule (see
[Choosing methods](#choosing-methods)). Cost is relative CPU per comparison: *low* is nanoseconds to
microseconds, *medium* microseconds to tens of microseconds, *high* needs a model or an external call.

### Normalizers (make more pairs exact)

Often the biggest win: after cleaning, many duplicates are plain equal values.

| Id | What it does |
|---|---|
| `lower`, `trim`, `collapse_space`, `strip_punct` | the basics |
| `fold_accents` | `é` to `e`, `ọ` to `o`, Unicode NFKD |
| `strip_titles` | Mr, Dr, Alhaji, Chief, Prof, Engr, … (locale lists, extendable) |
| `strip_company_suffix` | Ltd, Plc, Inc, GmbH, Nig., … |
| `digits` | keep digits only |
| `phone_e164` | canonical phone number using the country (`0803…` and `+234 803…` become one value) |
| `email_canonical` | lower-case; Gmail dots and `+tags` removed; optionally domain aliases |
| `tax_id`, `bank_account`, `national_id` | strip separators and check digits per country |
| `initials` | `John Obi` to also `J Obi`; the pair of forms are both keyed |
| `nickname` | `Bob` to `Robert` from the variant table (shared with phonetics) |
| `transliterate` | Arabic, Cyrillic, Greek to Latin; Arabic name variants (`Muhammad`, `Mohamed`) |
| `date_canonical` | one date form from several input forms |

### Blockers (find candidates without comparing everything)

| Id | Method | Cost | Good for | Phase |
|---|---|---|---|---|
| `exact` | the normalized value itself | low | email, phone, tax id, account numbers | 1 |
| `prefix` / `suffix` | first or last *n* characters | low | names, codes | 1 |
| `token_sorted` | sorted tokens, then a prefix | low | names with changed word order | 1 |
| `phonetic` | Soundex, Metaphone, Double Metaphone, NYSIIS, Cologne | low | names, spelling variants | 1 |
| `name_variant` | the variant and nickname table | low | Mohammed/Muhammad, Bob/Robert | 1 |
| `sorted_neighbourhood` | sort on a key, compare within a sliding window | low | fallback inside large blocks | 1 |
| `ngram` | shared character trigrams above a threshold (inverted index) | medium | typos, free text, addresses | 2 |
| `minhash_lsh` | MinHash signatures with locality-sensitive hashing | medium | very large tables, long text | 2 |
| `geo_cell` | geohash or H3 cell of coordinates (and neighbours) | low | places, branches, customers' addresses | 2 |
| `identifier_link` | records sharing a bank account, tax id or other id from a *related* model | medium | relational evidence | 3 |
| `opensearch` | candidates from the OpenSearch bridge (fuzzy, phonetic analyzer, more-like-this) | external | very large tables, free text | 3 |
| `embedding_knn` | nearest neighbours in a vector index (OpenSearch k-NN, or a local index) | high | same meaning, different wording | 4 |

Several blockers on one rule are combined as a **union** (a pair is a candidate if any recipe shares a key),
which is what the `block` list in the rule file already expresses; a recipe with several parts is an
**intersection** (`["name:phonetic", "birth_date"]`).

### Scorers (how alike two candidates are)

| Id | Method | Cost | Good for | Phase |
|---|---|---|---|---|
| `exact` | equal after normalization: 1 or 0 | low | identifiers | 1 |
| `edit` | Levenshtein or Damerau-Levenshtein, with early exit | low | short typos | 1 |
| `jaro_winkler` | Jaro-Winkler | low | names (common prefix counts) | 1 |
| `token_set` / `token_sort` | token overlap ignoring order and extra words | low | "Dangote Cement Plc" vs "Dangote Cement" | 1 |
| `jaccard` | token or n-gram set overlap | low | tags, long text | 1 |
| `ngram` | trigram Dice or cosine | low | typos in longer text | 1 |
| `name` | person or organization name: token-aware, handles order, initials, nicknames, titles | medium | people and companies | 1 |
| `date` | equal, within a tolerance, or day and month transposed | low | birth dates | 1 |
| `number` | equal, or within an absolute or percentage tolerance | low | amounts, quantities | 1 |
| `list` | overlap of two lists | low | phones, emails, tags per record | 1 |
| `phonetic` | equal phonetic codes as a bonus to another score | low | names | 1 |
| `address` | parse into street, number, city, postcode, country; compare the parts; abbreviation table (`Rd`, `St`, `Ave`) | medium | postal and street addresses | 2 |
| `geo` | distance between coordinates, as a score by radius | low | places | 2 |
| `relational` | shared related records count as evidence (same employer, same account) | medium | parties, employees | 3 |
| `embedding` | cosine similarity of vectors from a model | high | meaning, not spelling | 4 |

### Combining and learning

| Id | Method | What it does | Phase |
|---|---|---|---|
| `weighted_sum` | Fellegi-Sunter style weighted sum, as described above | the default: explainable, no training | 1 |
| `rules_first` | a rule can short-circuit: "same tax id means a match", "different country means never" | hard evidence over weights | 1 |
| `learned_weights` | fit the weights (and thresholds) from the reviewers' own `confirmed` and `not_duplicate` decisions (logistic regression) | tunes a rule to the data; the model is the same weights table, so it stays explainable | 3 |
| `active_review` | put the pairs the model is least sure about at the top of the review list | gets labels fastest | 3 |

### Embeddings, with care

`embedding` and `embedding_knn` are the only methods that need a model, so they are **opt-in at three
levels**: the deployment must configure an embedding provider, the organization must turn it on, and the rule
must name it. The provider is one of: a **local** model run inside the kernel (no data leaves; costs memory
and CPU), the OpenSearch k-NN feature (vectors computed by the model OpenSearch hosts), or an external
embedding API through a bridge. Only the fields named in the rule are embedded, and the provider choice is
shown in the review screen. Vectors are stored per record and rule version, so a rule change re-embeds.

### Choosing methods

Nothing is on by default except what a plugin's rule file names. The decision belongs to whoever owns the
data, at three levels (a lower level can narrow but not widen the one above):

1. **The plugin's rule file** names the methods it recommends for each field (as in the example above).
2. **The organization** can switch methods on or off and change weights and thresholds in a *Deduplication*
   settings screen, as an override stored in the database; it never edits the plugin's file.
3. **A run** (`dedup::scan`) can take `methods` for one-off experiments: "scan with only `exact` and
   `phonetic`" to compare results.

`dedup::methods` lists every method the kernel has, with role, cost, what it needs (a locale, an OpenSearch
bridge, an embedding provider) and whether it is **available** here, so a screen can offer only what works.
A rule that names an unavailable method is refused at load with the reason; at run time a method that has
become unavailable (OpenSearch down) is skipped, and the run says which one it skipped.

To help people choose, the engine ships **presets**: `people`, `companies`, `addresses`, `identifiers_only`,
`free_text`. A preset is just a rule fragment (normalizers, blockers, scorers, thresholds) the plugin can
extend with `"preset": "people"`. A *compare methods* run scores the same sample of reviewed pairs under
several method sets and reports how many known duplicates each finds and how many false pairs it adds, so the
choice is made on the organization's own data and not on a guess.

## How matching works

Comparing every record with every other is n squared (a million rows is 500 billion pairs). All the speed
comes from never doing that.

### 1. Normalize

Each field is cleaned by the listed steps before anything else: lower-casing, accent folding, whitespace,
punctuation, titles and company suffixes (`Mr`, `Alhaji`, `Ltd`, `Plc`), digits only, E.164 phone numbers.
Steps are small functions in the engine, applied in order and cached per record.

### 2. Blocking keys

Each record produces a handful of short keys from the recipes in `block`: an exact value, a phonetic code,
a name's sorted tokens' first four letters, a date. **Two records are candidates only when they share a key.**
Keys are 64-bit hashes of `(rule, recipe, value)`.

* **Phonetic codes** are a pluggable list: Soundex, Metaphone, Double Metaphone, NYSIIS, Cologne phonetics,
  and a name-variant table (Mohammed/Muhammad/Mohamed, Chukwu/Chukwuma) shipped as data and extendable
  by an organization. The codes are chosen by locale (`locale_field` or the organization's setting), because
  phonetics are language-specific. Beider-Morse and Arabic transliteration are later additions behind the
  same trait.
* **Oversized blocks** (a key shared by thousands of records, like the code of `Mohammed`) are not compared
  in full: the engine splits them with a second key, or applies sorted-neighbourhood inside the block with a
  fixed window, and reports that it did, so a person can see the rule is too loose.

### 3. Score

Inside a block, each candidate pair is scored field by field and combined as a weighted sum into 0 to 1,
the Fellegi-Sunter style: agreement adds the field's weight, a clear disagreement subtracts part of it, a
missing value on either side is neutral. The cheap tests come first (length difference, then a bit-parallel
Levenshtein or Jaro-Winkler with an early exit once the pair cannot reach `review`), and the per-field
scores are kept as the **explanation** shown to the person reviewing.

### 4. Cluster

Pairs at or above `review` are joined with union-find into groups. A group is only proposed when every
member is close to the group's best-linked record, so one bad link cannot chain two different people
together; looser groups are split and flagged.

### 5. Remember decisions

A person marking a pair *not duplicates* writes a `dedup_decision`; later scans skip that pair. Without it
the same false positive would come back at every scan.

## Two ways in

### Check on write: milliseconds

`dedup::check` takes one record (new or edited) and answers the likely duplicates. It computes the record's
keys and looks them up; it never scans a table.

To make the lookup cheap the keys are **stored**, in `dedup_key (rule, key, record)` with an index on
`(rule, key)`. The kernel writes a record's keys in the same transaction as the record, so the table is never
stale (an edit replaces them, a delete removes them). `check` is then one indexed read per key, then scoring
a few dozen candidates in memory. Target: under 20 ms for a million-row table, measured before we promise it.

A model can ask the kernel to run `check` itself on every create:

```json
{ "on_create": "warn" }      // or "block", or "off" (default)
```

`warn` returns the candidates with the create's answer so the screen can show "this looks like Ada Obi";
`block` refuses the create unless the caller passes `confirm_not_duplicate: true`. `block` is only offered
for rules whose `unique_evidence` fields matched (email, tax id), never for fuzzy matches.

### Scan: a background job

`dedup::scan` queues one run over a model. The stored keys make the first step a database query rather than
a table read: *"every key shared by more than one record"* is a group-by on `dedup_key`, which is exactly the
list of blocks. The run then:

1. Streams the blocks in pages (by key range), so memory is bounded however big the table is.
2. Hands each page to the engine's thread pool, which scores its blocks in parallel.
3. Writes the pairs at or above `review` and, at the end, the groups.
4. Records progress after every page (`dedup_run`), so it can be resumed, cancelled or watched.

Before the first scan of an existing table the keys must exist: the run begins with a **build keys** step
that streams the table once and writes them (also parallel). Changing a rule re-keys only what its recipes
affect, and a run records the rule's version so old results are not mistaken for new ones.

#### The scan runs in the kernel, not in a plugin job

The scheduler's job kinds are `plugin` and `communication`; we add `dedup`, executed natively by the worker.
It gets the scheduler's retries, queues, leases and audit like every other job, but none of the plugin
limits: no fuel, no 64 KiB payload, no 10 s. The job is **resumable** (progress is in the database) and the
lease is renewed after each page, so a worker that dies hands the run to another, which continues at the
last finished page. A page may be scored twice after a crash; writing a pair is an upsert on
`(rule, a, b)`, so repeating is harmless, which is what at-least-once delivery needs.

## Parallelism and not starving the server

True parallelism is the point, and also the risk: a scan on every core makes logins slow.

* The engine owns a **bounded rayon pool**, separate from the Tokio runtime. Page reads and writes stay
  async; scoring runs on the pool through `spawn_blocking`, never on Tokio's worker threads.
* Pool size is a setting, default **half the cores** (`dedup.threads`). Scans take the pool one run at a
  time per node (`dedup.concurrent_runs`, default 1); a second queues.
* Memory is bounded by a setting (`dedup.memory_mb`, default 512): page size and the largest block are
  derived from it, and the run reports when a block was cut to fit.
* The pool checks a cancel flag between blocks, so a cancel takes effect in well under a second.
* A scan is rate-aware of the database: page reads use the same connection pool as requests, so
  `dedup.read_batch` (default 2000 rows) and a short pause setting keep it from monopolizing it.

Speed targets (**goals, to be measured, not results**): 1M rows with a good rule in minutes on 8 cores;
`check` in tens of milliseconds. They will be written into the benchmarks of the engine crate (criterion,
in-memory, synthetic Nigerian/Arabic/European name sets with injected typos) and the numbers copied here
once real.

## OpenSearch: a bridge any plugin can use

Search is useful well beyond duplicates (find a patient by free text, search documents, autocomplete), so
the `opensearch` bridge is written as a **plain OpenSearch client** and dedup uses it the way a plugin would.

### The bridge

Like Paystack and OpenStreetMap it is called with `bridge::call` (see [Bridges](../bridges.md#calling-a-bridge-bridgecall)):

```toml
# plugin.toml
capabilities = ["bridge::call"]
bridges = ["opensearch"]
```

```rust
let hits: Value = bridge::call("opensearch", "search", &json!({
    "index": "patients", "query": { "match": { "name": { "query": "ada obi", "fuzziness": "AUTO" } } }, "size": 20
}))?;
```

* **Settings** (database, global with an organization override, secrets encrypted, never in `aether.toml`):
  `base_url`, `username`, `password` (secret) or `api_key` (secret), `verify_tls`, `request_timeout`.
  An organization with its own cluster fills them in; otherwise the global one is used, never a mix.
* **Actions** (generic, no dedup concepts):

  | Action | What it does |
  |---|---|
  | `ping` | cluster health and version; the settings screen's "test" button |
  | `ensure_index { index, mappings?, settings?, analyzers? }` | create the index, or update it when only additive changes are needed; answers whether it was created, changed or already right |
  | `index_docs { index, docs: [{ id, doc }], refresh? }` | bulk index (create or replace) |
  | `delete_docs { index, ids }` | bulk delete |
  | `search { index, query, size?, from?, sort?, fields?, highlight? }` | OpenSearch query DSL; answers `{ total, hits: [{ id, score, fields, highlight? }] }` |
  | `msearch { index, queries: [..] }` | several searches in one round trip (used by scans) |
  | `knn { index, field, vector, k, filter? }` | vector nearest neighbours, for embeddings |
  | `delete_index { index }` | remove an index the caller owns |

  Only these documented actions exist: no raw passthrough to the cluster, in line with "no raw SurQL".
  The provider's whole answer is not returned, only the fields above (see Bridges, last bullet).
* **Names are namespaced by the kernel, not trusted from the caller.** The `index` a plugin names is stored as
  `<prefix>.<org>.<plugin>.<index>`, so a plugin can reach only its own indexes, an organization only its own,
  and dedup's indexes (`<prefix>.<org>.dedup.<rule>`) are out of every plugin's reach. `index_prefix` is a
  setting so several Aether installs can share a cluster.
* **Limits** are the bridge's, set once, so no client has to know them: documents per `index_docs` call, bytes
  per call, `size` and `from + size` per search, and the 45 s `bridge::call` timeout. A caller that has more
  to index uses the [scheduler](scheduler.md) in chunks, as it already does for any large work.
* **Keeping an index current is the plugin's job.** A plugin that wants its model searchable calls
  `index_docs` from its own write functions or from a model [event](events.md), and runs a rebuild as
  a background job. (A later convenience could let a model declare `"search": { "fields": [...] }` and have
  the kernel keep the index in step, the way it keeps `dedup_key`. That is a separate decision and not part of
  this proposal.)
* **What leaves the building.** Documents are sent to an external service. That is the organization's choice:
  the bridge does nothing until configured, and a plugin sends only the fields it puts in `docs`.
* Logged with plugin, bridge, action, index and duration; never the query or documents.

### How dedup uses it

Dedup is a client in the kernel: its glue in `facets/core` implements `CandidateSource` on top of the same
bridge function plugins reach through `bridge::call`, with the same settings and the same namespacing (as
the `dedup` owner), and without the plugin capability check, since the caller is the kernel.

When the organization has configured the bridge, a rule may say `"source": "opensearch"` (or the setting
`dedup.source` makes it the default). Dedup then uses OpenSearch only to **produce candidates**; scoring and
clustering stay native, so results look and rank the same either way and the person can switch back.

```
check:  record → search (fuzzy / phonetic analyzer / more_like_this) or knn → candidate ids → native scoring
scan:   native keys still define the work; each page's records go in one msearch
```

* The index and its analyzers are **generated from the rule** (a phonetic analyzer per phonetic field) and
  created with `ensure_index`, one per organization and rule version, so a rule change reindexes and a stale
  index is never queried.
* Dedup keeps its index current from the same hook that writes `dedup_key`: a create or edit queues an
  `index_docs` job on the scheduler, so a slow OpenSearch never slows a write. `check` accepts that the index
  can be a few seconds behind and also looks at `dedup_key` for records written since.
* Without OpenSearch nothing changes: the stored keys are the candidate source. OpenSearch earns its place
  with very large tables, languages whose analyzers we do not have, and free-text fields (addresses,
  company names) where blocking keys do badly.
* Dedup sends only the fields named in the rule, never the whole record.

## Kernel commands

| Command | Capability | What it does |
|---|---|---|
| `dedup::check { model, record \| id, rule?, limit? }` | `dedup::check` | likely duplicates of one record: `[{ id, score, band, fields: { name: 0.96, … } }]` |
| `dedup::methods { model? }` | `dedup::check` | every method with role, cost, requirements and whether it is available here |
| `dedup::compare { model, rule, method_sets }` | `dedup::review` | score a sample of reviewed pairs under several method sets and report found, missed and false pairs |
| `dedup::scan { model, rule?, methods?, resume? }` | `dedup::scan` | queue a run; answers `{ run }` |
| `dedup::run { run }` | `dedup::scan` | state, progress, counts, blocks that were cut |
| `dedup::cancel { run }` | `dedup::scan` | stop a run; finished pages are kept |
| `dedup::groups { model, rule?, state?, limit, offset }` | `dedup::review` | proposed groups with members and explanation, **filtered by the caller's record rules** |
| `dedup::decide { group \| pair, decision }` | `dedup::review` | `not_duplicate`, `confirmed`, `merged` (see merge below) |

All need the model in the plugin's granted models, and write the usual audit rows. A plugin may only
declare rules for models it owns or has been granted.

**Access.** A scan runs as the kernel (`system:dedup`) like scheduler jobs, so it sees every row; what a
person sees is decided on the way out: `groups` and `check` drop members the caller may not read under the
model's [record rules](rules.md), and a group left with fewer than two visible members is not shown. A
duplicate list must not become a way to learn that a hidden record exists.

## Tables

| Table | Holds |
|---|---|
| `dedup_rule` | a plugin's rule as loaded, with a version hash |
| `dedup_key` | `(rule, key, record)`, indexed on `(rule, key)` and on `record` |
| `dedup_run` | a run: model, rule version, state, page cursor, counts, who started it, errors |
| `dedup_pair` | `(rule, a, b)`, score, per-field scores, run |
| `dedup_group` / `dedup_member` | the proposed groups and their members |
| `dedup_decision` | `not_duplicate`, `confirmed`, with who and when |

All live in the organization's database (tenancy as everywhere); the engine's phonetic tables and variant
lists are compiled in, with organization additions in settings.

## Settings

A **Deduplication** group in [Settings](settings.md), seeded like the others: `dedup.threads`,
`dedup.concurrent_runs`, `dedup.memory_mb`, `dedup.read_batch`, `dedup.source` (`native` or `opensearch`),
`dedup.max_block_size`, `dedup.methods` (organization on/off per method), `dedup.embedding_provider`, `dedup.retention_days` for old runs and pairs. OpenSearch's own are `bridge.opensearch.*`.

## Merging: later, and separate

Detecting duplicates is safe; merging changes data. It is **out of the first version.** When built it is
a `dedup::merge { survivor, losers, field_choices }` that:

1. shows a **plan** first: which records point at the losers (every `link` field of every model in the
   catalog, which the kernel already knows, see [Models](models.md#links-to-another-plugins-model)), and
   which field values the survivor takes from which record;
2. applies it in one transaction per model batch, repointing links, merging graph edges, then archiving (not
   deleting) the losers so it can be undone;
3. announces `dedup.merged` as a plugin [event](events.md) so a plugin can fix what the kernel cannot know.

Plugins decide for themselves what to do about records the kernel cannot repoint.

## Not in scope

* Matching **across** models (a lead against a party). One model per rule at first.
* Fuzzy matching of **files and images**.
* Opaque matching. Every method, including embeddings and learned weights, must still report **why** a pair
  scored as it did (per-field scores and evidence), so a person can see why two records were grouped.
* Real-time reindexing on OpenSearch as a hard guarantee.

## Build order

Each step is useful alone; step 2 already gives "possible duplicate" on create.

1. **Engine, phase 1 methods**: the three traits, all phase 1 normalizers, blockers and scorers, `weighted_sum`
   and `rules_first`, clustering, unit tests and a benchmark harness. No kernel yet.
2. **Rules and keys**: rule file and its load-time checks, presets, `dedup_key` kept in step with writes,
   `dedup::check`, `dedup::methods`.
3. **Scan**: scheduler job kind, resumable runs, tables, `groups`/`decide`, the settings group with the
   per-organization method switches, the review screen.
4. **Phase 2 methods**: `ngram` and `minhash_lsh` blockers, `address` and `geo` scorers, `geo_cell`;
   `dedup::compare`.
5. **OpenSearch bridge** (general actions, namespacing, settings screen, available to plugins from day one) and then dedup's `CandidateSource` on top of it (`opensearch` blocker).
6. **Phase 3 methods**: `relational` scoring and `identifier_link`, `learned_weights`, `active_review`.
7. **Phase 4: embeddings**: provider choice, vector storage, `embedding` and `embedding_knn`.
8. **Merge** with plan, undo and the event.

## Decide

| # | Question | Proposal |
|---|---|---|
| 1 | Facet or plugin | **Facet** (`facets/dedup`), engine kept free of kernel types, glue in `facets/core` |
| 2 | OpenSearch as a bridge or inside the facet | **A general bridge** (`bridges/search/opensearch`) with generic actions and kernel-namespaced indexes, usable by any plugin through `bridge::call`; dedup is one client, through its `CandidateSource` trait |
| 2b | Should the kernel keep a plugin's model searchable automatically (`"search"` in the model) | Not in this proposal; plugins call `index_docs` themselves first, and we add it if several plugins repeat the same code |
| 3 | Persist blocking keys in the database (`dedup_key`) or keep the index only in memory | **Persist**: makes `check` an indexed read and a scan a group-by, survives restarts, costs one extra write per record. The alternative is a faster write path but a cold start that rebuilds everything |
| 4 | Scan as a native scheduler job kind, or as plugin jobs | **Native kind** `dedup`; plugin jobs cannot carry the work |
| 5 | Rules as plugin-shipped JSON, or created by admins in the web app | Plugin-shipped first; an admin screen can edit the same data later |
| 6 | `on_create`: `warn` / `block` | Offer both, `block` only for unique-evidence matches |
| 7 | Phonetic set for v1 | Soundex, Double Metaphone, NYSIIS, Cologne, plus the name-variant table; Beider-Morse later |
| 8 | Show duplicates the caller cannot read | **No**, filter by record rules |
| 9 | Merge in v1 | **No**, after review of the detection quality |
| 10 | Cross-model matching | Not in v1 |
| 11 | Default thread budget | Half the cores; one run per node at a time |
| 12 | Methods | **Build all of the [catalog](#matching-methods-the-catalog)**, in the phases shown; the owner of the data chooses per rule and per organization, nothing beyond the plugin's rule is on by default |
| 13 | Embeddings | Opt-in at three levels (deployment, organization, rule); local model preferred over an external API |
| 14 | Learned weights | Keep the weights table as the model so every score stays explainable; train only on the organization's own decisions |
| 15 | Method choice help | Ship presets and a `dedup::compare` run, so the choice is measured on the organization's data |
