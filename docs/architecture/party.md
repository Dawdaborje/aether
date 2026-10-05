# Party: the people and organizations you deal with

A **party** is anyone you do business with or keep a record of: a customer, a supplier, an employee's
contact card, a bank, a school, the company itself. It is the equivalent of Odoo's `res.partner`: one record per
real-world person or organization, referenced by invoices, leads, contracts, payslips and everything else that
needs "who".

It lives in the **base** workspace (`plugins/base/party`), written in Rhai, together with the plugins it builds
on and the one that builds on it:

```
currency  ──►  country  ──►  party  ──►  company
(currencies)   (countries)   (people and organizations)   (legal entities)
```

Load them in that order (`aether --load-plugin plugins/base/currency plugins/base/country …`); installing
`company` in an organization installs the other three first.

## Why a plugin, not the kernel

The kernel owns *login accounts* (users), tenancy and permissions. A user is someone who can sign in; a party is
someone the business has a record of. Most parties never sign in, and a user need not be a party. The two are linked
by an optional reference (`party.user`), so the kernel stays unaware of business data. A plugin can be replaced,
extended and versioned; the kernel cannot.

## How plugins link to each other

A `link` field may point at **another plugin's model** (`"target": "currency.currency"`). It is how `party`
refers to a currency, `party_address` to a country and `company` to its party. See
[Models](models.md#links-to-another-plugins-model). The kernel checks that the linking plugin lists the other
as a dependency, that the model exists in the catalog under the id recorded in the field, and that every value
written points into that model's table.

## What exists

### `currency` (v0.3)

`currency` (code, name, symbol, decimal places, base, active) and `currency_rate` (a currency's rate against the
base currency on a date). Functions: `create_currency`, `update_currency`, `list_currencies`, `set_active`,
`set_base`, `get_base`, `set_rate`, `get_rate`, `list_rates`, `convert`, `seed_currencies` (65 currencies in common
use; the first run makes USD the base), `get_currency_by_code`. Converting from A to B on a date is
`amount / rate(A) * rate(B)` with the newest rate on or before the date, rounded to B's decimals.
Command: `currency.seed`. Announces `currency.rate_changed`.

### `country` (v0.1)

`country` (ISO code, 3-letter code, name, phone prefix, usual **currency**, active) and `country_region`.
Functions: `create_country`, `update_country`, `get_country`, `list_countries`, `add_region`, `list_regions`,
`remove_region`, `seed_countries` (the 54 African countries and 30 main economies elsewhere; each linked to its
currency). Command: `country.seed` (run `currency.seed` first).

### `party` (v0.1)

| Model | Holds |
|---|---|
| `party` | `kind` (person or organization), `name`, `display_name` (kept up to date: a person who belongs to an organization shows as `Ann Okoro (Acme Ltd)`), `email` (stored lower case), `phone` and `mobile` (international form), `website`, `tax_id`, `language`, `currency` (link), `parent` (link to the organization a person belongs to), `job_title`, the role flags `is_customer`, `is_supplier`, `is_employee`, `is_bank`, `user` (the login account, if any), `notes`, `is_active`. Chatter is on and the main fields are tracked. |
| `party_address` | `party`, `type` (billing, shipping, home, office, other), street lines, city, region, postal code, `country` (link), `is_default`, optional latitude and longitude |
| `party_bank_account` | `party`, bank, account number, IBAN (stored upper case), SWIFT, `currency` (link), holder, `is_default` |
| `party_tag`, `party_tag_link` | free-form labels and their assignment |

Functions:

| Function | Does |
|---|---|
| `create_person`, `create_organization` | validate and normalize (lower-case email, international phone, trimmed names), refuse a tax number another party has, compute `display_name`, create; announce `party.created` |
| `update_party` | the same checks on a change; renaming an organization refreshes its people's display names; announce `party.updated` |
| `get_party`, `get_party_details` | the record; or with its addresses, contacts and tags (bank accounts are never in the details) |
| `search` / `list_parties` | by text (name, email, phone, tax number), kind, role or tag; active parties unless asked; at most 500 results |
| `archive`, `unarchive` | `is_active`. Parties are never deleted because other records point at them. Archiving an organization with active contacts is refused unless `with_contacts` says to archive them too; announces `party.archived` |
| `add_contact` | a person who belongs to an organization |
| `add_address`, `set_default_address`, `list_addresses`, `remove_address` | the first address of a type is its default; naming a default switches the old one atomically |
| `add_bank_account`, `list_bank_accounts`, `remove_bank_account` | needs an account number or IBAN |
| `add_tag`, `tag_party`, `untag_party`, `list_tags` | |
| `link_user` | ties a party to a login account (one party per account) |
| `find_duplicates` | groups active parties that share a tax number, an email or a phone number |

Command: `party.find_duplicates`. Mistakes come back as plain sentences ("`not-an-email` is not an email address",
"the tax number TIN-1 is already used by Acme Ltd"), including when they happen in a plugin it called.

### `company` (v0.1)

`company`: `name`, `party` (a link to the legal entity's party record, which holds the name, tax number, addresses and
bank accounts), `currency` (link), the fiscal year's first month and day, a logo, `is_default`, `is_active`.
Functions: `create_company` (also creates the party record; the first company becomes the default),
`update_company` (renaming also renames the party), `get_company`, `list_companies`, `get_default_company`,
`set_default_company` (atomic), `archive_company` (not the default one), and `fiscal_year` (the fiscal year that
contains a date, for the default company or one named: `{ start, end }`, both included; a Feb-29 start moves to the
28th in years without one). Command: `company.create`. Having several companies in one organization is possible;
**scoping other plugins' data by company** is a larger design (record rules per company) and is not done.

## How other plugins build on it

| Need | Do this |
|---|---|
| Refer to a party | a `link` field targeting `party.party`; list `party` under `dependencies` |
| A role with its own life-cycle and many records (an employee with contracts, a customer with credit terms) | its own model that links to the party |
| Attributes that belong to a party in one role | its own model linking to the party (one record per party), until model extends exist |
| React to a party changing | listen to `party.created`, `party.updated` or `party.archived` (`[[events]]` with `direction = "listen"`, see [Events](events.md)) |
| Create or find parties from code | `plugins::invoke("party", "create_person", …)`, `plugins::invoke("party", "search", …)` |

## Where it differs from the first design, and what is not done

* **Roles are flags**, not a multi-select, because the model system has no multi-value field and filtering
  needs equality. The four roles are fixed; a plugin that needs another role models it itself.
* **Model extends are not built.** The first design had other plugins add fields to `party`'s table. Until
  extends exist, they link to a party from a model of their own (see the table above).
* **No separate permission for bank accounts.** Any member who can call the party functions can call
  `list_bank_accounts`; the plugin permission checks the first design assumed (`party.bank.read`) are not
  enforced by the kernel yet. They are kept out of `get_party_details` so they are never shown by accident.
* **Search scans up to 1000 parties** matching the filters and matches text in the script, because
  `db::find` filters by equality only.
* **Links are checked for shape and table, not for existence.** A link must point into the right model's
  table, as with every link, but the kernel does not check that the record is there.
* **Merging duplicates** (repointing every reference from one party to another) is not built; `find_duplicates`
  only reports.
* **Personal data.** Access is audited like all data access. Parties are archived, never deleted, so a "right to
  be forgotten" request is an explicit action (blank the personal fields of the record), not a delete that would
  break invoices.
