# Developer Guide

## Introduction

This document provides a guide for developers who wish to contribute to the Game-save-manager project. It includes information about the project's goals, architecture, and development process.

## How to Develop Locally

### Environment Setup

You need to have the following environments pre-installed:

- [Node.js](https://nodejs.org/) and [pnpm](https://pnpm.io/)
- [Rust Compiler Environment](https://www.rust-lang.org/) and Cargo

### Editors and Plugins

- Visual Studio Code (Recommended)
  - Rust-analyzer
  - Tauri
  - Vue - Official
  - Element Plus Snippets
  - i18n Allay
- WebStorm
- RustRover

### Installing Dependencies

`pnpm i`

### Compilation and Development

Refer to `package.json` for instructions

- `pnpm dev` Development mode, preview while developing
- `pnpm build` Compile and package, output will be stored in `src-tauri/target`

## Architecture

The software is divided into two main parts:

- The frontend is responsible for the user interface and interactions. It is written in TypeScript and Vue3
  - Uses the Element Plus component library
  - Uses pinia for state management
  - Uses vue-router for frontend routing
  - Uses vue-i18n for internationalization
- The backend is responsible for managing game save files. It is written in Rust
  - Uses opendal to access cloud storage
  - Uses serde for serialization and deserialization of data
  - Uses thiserror and anyhow for error handling

## Development Process

To contribute to the Game-save-manager project, you need to:

1. Fork the repository's `dev` branch on GitHub
2. Clone the forked repository to your local machine
3. Create a new branch for your changes, such as `feat/webdav-essentials`
4. Make changes to the code and commit your changes to your local branch
5. Push your changes to your forked repository on GitHub
6. Create a pull request to merge your changes into the main repository's `dev` branch. Note that you always need to merge code in a rebase manner

### Merging Upstream Updates

After developing for a while, you may find that the upstream code has been updated. To keep your branch in sync with the upstream code, you can use the following commands:

```bash
git switch dev
git pull
git switch <your-branch>
git rebase dev
```

This way, we can keep the commit history clean and avoid unnecessary conflicts. However, if there are already conflicts, you need to resolve them manually. In this case, we recommend using squash merge to merge the code.

## Using `vue-devtools`

First, you need to install devtools and start it correctly

```bash
pnpm add -g @vue/devtools@next
vue-devtools
```

Next, please find `index.html` in the project root directory and add the following content in the `<head>` tag

```html
<script src="http://localhost:8098"></script>
```

## Coding Style

There is no complete coding style document for now. If you can help complete this part of the document, I would be very grateful. For now, please refer to the rest of the code, try to keep it concise, and leave appropriate documentation.

## Commit Messages

Please follow [Conventional Commits](https://www.conventionalcommits.org/) to write commit messages. This will help with collaboration and automated builds. You can use the VSCode plugin `Conventional Commits` to assist in writing your commit messages.

## Version Number Explanation

The version number format is `x.y.z`, where `x` is the major version number, `y` is the minor version number, and `z` is the revision number. Changes in `x` are likely to cause incompatible changes, changes in `y` may be important feature updates, and changes in `z` are just minor changes. Generally, the latter two can be automatically upgraded.

### Changes Required for Updates

Other developers do not need to change the version number, just add their update content to the changelog. The version number will be modified by the Maintainer when merging into the main branch.

- Update the version number in `src-tauri\Cargo.toml`

## Folder Explanation

- doc: Development documentation
- public: Static files
- scripts: Scripts for Github Action
- src: Source code for the frontend project
  - assets: Static resources
  - locales: Internationalization resources
  - schemas: Data format for saving
  - Others, please refer to the folder name
- src-tauri: Root directory for the backend project
  - src: Source code for the backend project

*This document was translated by Deepseek and manually proofread.*
