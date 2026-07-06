# Aether Database design

As I am deciding to make aether database driven archietecture and try as much as possible to store the entire application state in the database.

## Settings Tables

This setting is to store settings values for configuration and it is designed to be flexible to add new settings.

The settings is divided into 2:

- Global Settings : This is a settings that can work across multiple organizations. (table name: gl_settings)
- Organization Settings : This is a settings that is specific to an organization. (table name: settings)
