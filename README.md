# Site Manager

<div align="center">

![Site Manager](https://img.shields.io/badge/Electron-App-blue?style=for-the-badge&logo=electron)
![React](https://img.shields.io/badge/React-19.2.0-61DAFB?style=for-the-badge&logo=react)
![TailwindCSS](https://img.shields.io/badge/TailwindCSS-3-38B2AC?style=for-the-badge&logo=tailwind-css)
![License](https://img.shields.io/badge/License-MIT-green?style=for-the-badge)

**A modern, minimalist Electron application for managing local development sites with Nginx, SSL, and PHP-FPM**

[Features](#features) • [Installation](#installation) • [Usage](#usage) • [Development](#development) • [License](#license)

</div>

---

## 📋 Overview

Site Manager is a desktop application that simplifies the creation and management of local development sites. It automatically configures Nginx, generates SSL certificates, manages PHP versions, and handles proxy configurations for Node.js applications.

### ✨ Key Features

- 🚀 **Quick Site Creation** - Create PHP or proxy sites in seconds
- 🔒 **Automatic SSL** - Self-signed certificates with trusted Root CA
- 🎨 **Modern UI** - Beautiful interface with dark/light theme support
- 🔄 **View Toggle** - Switch between grid and list layouts
- ⚙️ **PHP Version Management** - Easily switch between installed PHP versions
- 🔧 **Site Configuration Editing** - Update PHP versions or proxy ports on the fly
- 📦 **Easy Distribution** - AppImage and .deb packages available

## 🎯 Features

### Site Management
- **PHP Sites**: Automatic Nginx + PHP-FPM configuration
- **Proxy Sites**: Reverse proxy for Node.js/Express applications
- **SSL Certificates**: Trusted self-signed certificates for HTTPS
- **Edit Configuration**: Change PHP version or proxy port after creation
- **Certificate Regeneration**: Refresh SSL certificates per site or globally

### User Interface
- **Dark/Light Theme**: Toggle between themes with preference persistence
- **Grid/List View**: Choose your preferred layout for viewing sites
- **Responsive Design**: Works beautifully at any window size
- **Modern Components**: shadcn/ui-inspired design with TailwindCSS
- **Icon Integration**: Lucide React icons throughout

### Developer Experience
- **Automated Releases**: GitHub Actions workflow for building releases
- **TypeScript**: Full type safety across the codebase
- **Hot Reload**: Fast development with Vite
- **Cross-platform**: Built with Electron for Linux (Windows/macOS support possible)

## 📦 Installation

### From Releases

Download the latest release from the [Releases page](https://github.com/danidoble/site-manager/releases):

**AppImage (Portable):**
```bash
chmod +x Site\ Manager-*.AppImage
./Site\ Manager-*.AppImage --no-sandbox
```

**Debian/Ubuntu (.deb):**
```bash
sudo dpkg -i site-manager_*_amd64.deb
```

### Build from Source

**Prerequisites:**
- Node.js 20 or higher
- npm or bun

**Steps:**
```bash
# Clone the repository
git clone https://github.com/danidoble/site-manager.git
cd site-manager

# Install dependencies
npm install

# Run in development mode
npm run dev

# Build for production
npm run build
```

## 🚀 Usage

### First Run

On first launch, Site Manager will check for required dependencies:
- Nginx
- PHP-FPM
- OpenSSL
- libnss3-tools (certutil)

If any are missing, click "Install Dependencies" to install them automatically (requires sudo).

### Creating a Site

1. Click **"New Site"** button
2. Enter a domain name (e.g., `myproject.local`)
3. Choose site type:
   - **PHP Site**: For Laravel, WordPress, or any PHP application
   - **Proxy**: For Node.js, Express, or other applications
4. Configure:
   - **PHP**: Select PHP version
   - **Proxy**: Enter port number
5. Click **"Create Site"**

The site will be automatically configured with:
- Nginx virtual host
- SSL certificate (trusted)
- `/etc/hosts` entry
- Document root at `/var/www/{domain}/public`

### Editing a Site

1. Click the **Edit** icon on any site card
2. Modify the PHP version or proxy port
3. Click **"Update Site"**

The Nginx configuration will be regenerated and reloaded automatically.

### Switching Views

- Click the **Grid/List** toggle button in the header
- **Grid View**: Visual cards with hover effects
- **List View**: Compact table layout

Your preference is saved automatically.

### Theme Toggle

Click the **Sun/Moon** icon to switch between dark and light themes.

## 🛠️ Development

### Project Structure

```
site-manager/
├── electron/
│   ├── main/
│   │   ├── index.ts          # Electron main process
│   │   ├── siteManager.ts    # Site management logic
│   │   └── store.ts          # Persistent storage
│   └── preload/
│       └── index.ts          # Preload script
├── src/
│   ├── components/
│   │   ├── ui/               # Reusable UI components
│   │   └── ThemeToggle.tsx   # Theme switcher
│   ├── contexts/
│   │   └── ThemeContext.tsx  # Theme provider
│   ├── lib/
│   │   └── utils.ts          # Utility functions
│   ├── App.tsx               # Main application
│   ├── main.tsx              # React entry point
│   └── index.css             # Global styles
├── .github/
│   └── workflows/
│       └── release.yml       # CI/CD workflow
└── package.json
```

### Tech Stack

- **Frontend**: React 19, TailwindCSS, Lucide React
- **Backend**: Electron, Node.js
- **Build**: Vite, TypeScript, electron-builder
- **State**: React hooks, localStorage
- **Styling**: TailwindCSS with custom theme

### Available Scripts

```bash
npm run dev      # Start development server
npm run build    # Build production bundle
npm run lint     # Run ESLint
npm run preview  # Preview production build
```

### Creating a Release

1. Commit all changes
2. Create and push a version tag:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```
3. GitHub Actions will automatically:
   - Build the application
   - Create AppImage and .deb packages
   - Create a GitHub release
   - Upload artifacts

## 🔧 Configuration

### Site Storage

Sites are stored in Electron's user data directory using `electron-store`:
- **Linux**: `~/.config/site-manager/config.json`

### SSL Certificates

- **Root CA**: `{userData}/ssl/rootCA.pem`
- **Site Certs**: `/etc/ssl/certs/{domain}.crt`
- **Private Keys**: `/etc/ssl/private/{domain}.key`

### Nginx Configuration

- **Sites Available**: `/etc/nginx/sites-available/{domain}`
- **Sites Enabled**: `/etc/nginx/sites-enabled/{domain}`
- **Alternative**: `/etc/nginx/conf.d/{domain}.conf`

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes (`git commit -m 'Add some AmazingFeature'`)
4. Push to the branch (`git push origin feature/AmazingFeature`)
5. Open a Pull Request

## 📝 License

This project is licensed under the MIT License - see the [LICENSE.md](LICENSE.md) file for details.

## 🙏 Acknowledgments

- [Electron](https://www.electronjs.org/) - Cross-platform desktop apps
- [React](https://react.dev/) - UI library
- [TailwindCSS](https://tailwindcss.com/) - Utility-first CSS framework
- [shadcn/ui](https://ui.shadcn.com/) - Design inspiration
- [Lucide](https://lucide.dev/) - Beautiful icons

## 📧 Contact

**danidoble** - [@danidoble](https://github.com/danidoble)

Project Link: [https://github.com/danidoble/site-manager](https://github.com/danidoble/site-manager)

---

<div align="center">
Made with ❤️ by danidoble
</div>
