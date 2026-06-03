# Aether

The word `Aether` comes from the Greek god of light. Sometimes spells as `Aither` or `Ether`

This is gonna be a rebuild of odoo in rust:

## Techinical design

-   Plugin: A plugin will be a set of rules that we will be using as business cases. A plugin will consist of wasm (business logic and other stuff like views and more.)
-   Database: For the database, I am going with `surreal db`.
-   Security: I am heavily inspired by tauri's security and I will go beyond
