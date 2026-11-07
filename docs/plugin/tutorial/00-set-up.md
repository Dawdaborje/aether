---
title: "Chapter 0: Set up"
description: "Install the aether binary, start a database, create the first administrator and open the dev server."
section: plugin
status: stable
order: 12
---
# Chapter 0: Set up

**Goal:** a running Aether you can sign in to. Everything later happens in this folder.

## 1. What you need

- The `aether` program: one binary that is both the command line and the server.
- [SurrealDB](https://surrealdb.com/docs/running/installation), the database.

See [Prerequisite](../../prerequisite.md) for the details. Check that both are on your path (each should print something rather than "command not found"):

```sh
aether --help
surreal version
```

## 2. Make a working folder and start the database

```sh
mkdir tasks-tutorial && cd tasks-tutorial
surreal start rocksdb://surrealdb --user root --pass root
```

If you already have a SurrealDB running, skip the second command and make sure `[database]` in the next step points at it. Leave that running in its own terminal. The data lives in the `surrealdb` folder next to you, so you can stop it and
come back later.

## 3. Write the configuration

In a second terminal, in the same folder:

```sh
aether -q aether.toml
```

This writes `aether.toml` with sensible defaults. For this tutorial the only change is to turn on development mode,
so the server reloads plugins when files change. Open the file, find this setting under `[configuration]` and change
`false` to `true` (do not add a second `[configuration]` table; a file may have each table only once):

```toml
is_development_mode = true
```

The defaults already point at SurrealDB on `localhost:8000` with user and password `root`, and at port `7890` for the
web server.

## 4. Create the first administrator

```sh
aether -c aether.toml --init --admin-username admin --admin-password 'choose-a-password'
```

`--init` prepares the database, adds the standard settings and creates your administrator. A fresh install has **one
organization**, called `main`, which `--init` creates for you. Everything in this tutorial happens in it. (See
[Organizations](../../architecture/organizations.md).)

## 5. Start the server

```sh
aether -c aether.toml --serve --watch
```

`--watch` reloads a plugin when you change one of its files. The server answers the API and the documentation: open
<http://localhost:7890/docs> to check that it is up.

## 6. The web app

The screens you will use from chapter 3 on (the Apps screen, lists, forms) are drawn by the web app. It is a separate
project, the `web/` folder of the Aether repository, that talks to the server:

```sh
cd web
npm install
npm run dev
```

Open the address it prints (<http://localhost:5173>) and sign in as `admin`. It forwards `/api` to the server on port
`7890`; if your server listens elsewhere, set `PUBLIC_API_BASE_URL` to its address before `npm run dev`. You should see the
Apps screen, empty or with a few built-in tiles. This is where Tasks will appear. Chapters 0 to 2 need only the command
line, so you can leave this until chapter 3.

> **Stuck?**
> - *connection refused* when running `--init`: SurrealDB is not running, or not on port 8000 (check `[database]` in
>   `aether.toml`).
> - *`/docs` does not load*: look at the terminal where the server runs; it prints the address it listens on.
> - *the web app shows a network error*: it cannot reach the server; check `PUBLIC_API_BASE_URL` and that the server is running.
> - *forgot the password*: `aether -c aether.toml --change-password admin --password new-secret`.

Next: [Chapter 1, Hello, plugin](01-hello.md).
