# Aether Database design
As I am deciding to make aether database driven archietecture and try as much as possible to store the entire application state in the database.
The database in choice here is **surrealdb**, and the reason why is that surrealdb has 3 types of database baked into 1, and was very easy to switch database which aether was design to be table based tenancy.

## Core Table Design
These are the core tables that store the application state. And are always in the main database rather than the tenant database(s).

### Organization Table (Tenant)
This table stores the organization details and is specific to each tenant.

### Company Table
This table stores the company details and is specific to each tenant.

### Settings Tables
This setting is to store settings values for configuration and it is designed to be flexible to add new settings.
The settings is divided into 2:

- Global Settings : This is a settings that can work across multiple organizations. (table name: gl_settings)
- Organization Settings : This is a settings that is specific to an organization. (table name: settings)
