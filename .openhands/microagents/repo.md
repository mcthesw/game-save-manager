# Game Save Manager Repository Guide

## Project Description

Game Save Manager is an open-source desktop application that helps users manage their game save files with a user-friendly graphical interface. Built with modern web technologies and Rust, it provides features like cloud backup (WebDAV), scheduled backups, quick operations, and tray shortcuts. The application is designed to be lightweight with minimal system resource usage while offering comprehensive save file management capabilities.

## Technology Stack

- **Frontend**: Vue 3, TypeScript, Element Plus, Nuxt 3
- **Backend**: Rust with Tauri framework
- **Build System**: pnpm for Node.js dependencies, Cargo for Rust
- **Internationalization**: vue-i18n with Weblate integration
- **Cloud Storage**: OpenDAL for WebDAV support

## File Structure Overview

```
├── src/                    # Frontend Vue.js application
│   ├── components/         # Reusable Vue components
│   ├── composables/        # Vue composition functions
│   ├── layouts/            # Page layouts
│   ├── pages/              # Application pages/routes
│   └── public/             # Static assets
├── src-tauri/              # Rust backend application
│   ├── src/                # Rust source code
│   │   ├── backup/         # Backup functionality
│   │   ├── cloud_sync/     # Cloud synchronization
│   │   ├── config/         # Configuration management
│   │   └── quick_actions/  # Tray shortcuts
│   └── capabilities/       # Tauri security capabilities
├── doc/                    # Developer documentation (en/zh-CN)
├── locales/                # Translation files
├── scripts/                # Build and automation scripts
└── .github/                # GitHub workflows and templates
```

## Development Commands

### Prerequisites
- Node.js and pnpm
- Rust compiler and Cargo

### Setup
```bash
pnpm install                # Install dependencies
```

### Development
```bash
pnpm dev                    # Start development server with hot reload
pnpm web:dev                # Frontend-only development
pnpm build                  # Build production application
pnpm portable               # Create portable version
```

### Testing
The project uses GitHub Actions for CI/CD with workflows for:
- Rust Clippy linting
- Tauri builds for multiple platforms
- Pre-release and release automation

## Development Workflow

1. Fork the `dev` branch (not main)
2. Create feature branches from `dev`
3. Follow Conventional Commits for commit messages
4. Use rebase workflow to maintain clean history
5. Submit pull requests to `dev` branch

## Additional Information

- **Internationalization**: Uses Weblate for community translations
- **Platform Support**: Windows (Win7+), macOS, Linux
- **Dependencies**: Requires WebView2 on Windows
- **Documentation**: Comprehensive guides available in `doc/` directory
- **Community**: Active QQ group (837390423) and GitHub Discussions

For detailed development setup and contribution guidelines, refer to `doc/en/README.md` or `doc/zh-CN/README.md`.