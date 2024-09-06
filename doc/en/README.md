## Developer Guide

## Introduction

This document provides a guide for developers who want to contribute to the Game-save-manager project. It includes information on the project's goals, architecture, and development process.

## Local Development Setup

### Environment Setup

You will need the following environment installed beforehand:

- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/)
- [Rust compiler](https://www.rust-lang.org/) and Cargo

### Editors and Plugins

- Visual Studio Code (recommended)
  - Rust-analyzer
  - Tauri
  - Vue - Official
  - Element Plus Snippets
  - i18n Allay
- WebStorm
- RustRover

### Install Dependencies

`pnpm i`

### Build and Development

Refer to `package.json` for commands

- `pnpm tauri:dev` Development mode, preview while developing
- `pnpm tauri:build` Build and package, output will be stored in `src-tauri/target`

## Architecture

The software is divided into two main parts:

- Frontend is responsible for the user interface and interaction. It is written in TypeScript and Vue3
  - Uses Element Plus component library
  - Uses pinia for state management
  - Uses vue-router for frontend routing
  - Uses vue-i18n for internationalization
- Backend is responsible for managing the game save files. It is written in Rust
  - Uses opendal for cloud storage access
  - Uses serde for data serialization and deserialization
  - Uses thiserror and anyhow for error handling

## Development Process

To contribute to the Game-save-manager project, you will need to:

1. Fork the `dev` branch of the repository on GitHub
2. Clone the forked repository to your local machine
3. Create a new branch for your changes, such as `feat/webdav-essentials`
4. Make your changes to the code and commit your changes to your local branch
5. Push your changes to your forked repository on GitHub
6. Create a pull request to merge your changes into the `dev` branch of the main repository

## Using `vue-devtools`

First, you need to install the devtools and start them correctly.

```bash
pnpm add -g @vue/devtools@next
vue-devtools
```

Next, please find the `index.html` in the project root directory and add the following content inside the `<head>` tag.

```html
<script src="http://localhost:8098"></script>
```

## Coding Style

There is no formal coding style document at the moment, if you could help complete this part of the documentation I would be very grateful, for the time being please refer to the rest of the codebase, try to keep it concise and leave proper documentation

## Commit Messages

Please follow [Conventional Commits](https://www.conventionalcommits.org/) when writing your commit messages, this helps with collaboration and automated builds, you can use the VSCode extension `Conventional Commits` to help you write your commit messages

## Versioning

The version number follows the format `x.y.z`, where `x` is the major version, `y` is the minor version, and `z` is the patch version. Breaking changes will most likely result in a change to `x`, important feature updates may be a change to `y`, and `z` changes are just small changes, the latter two can usually be upgraded automatically.

### Changes you need to make for updates

Other developers do not need to change the version number, just add your updates to the changelog. The version number will be changed by a Maintainer when merging to the main branch.

- Update the version number in `package.json`(`pnpm version <patch|minor|major>`)
- Update the version number in `src/schemas/saveTypes.ts`
- Update the version number in `src-tauri/src/config.rs:default_config`
- Update the version number in `src-tauri/tauri.conf.json`

## Folder Structure

- doc: Development documentation
- public: Static files
- scripts: Scripts used for Github Actions
- src: Source code for the frontend project
  - assets: Static assets
  - locales: Internationalization resources
  - schemas: Schemas for saving data
  - Refer to folder names for others
- src-tauri: Root directory for the backend project
  - src: Source code for the backend project
