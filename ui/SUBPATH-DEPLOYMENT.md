# Subpath Deployment Guide

This guide explains how to deploy the CleverCoffee UI under a subpath like `/ui/` instead of the root path `/`.

## Quick Setup

### 1. For Development with Subpath

```bash
# Start dev server with /ui/ base path
npm run dev:ui

# The app will be available at: http://localhost:5173/ui/
```

### 2. For Production Build

```bash
# Build for deployment under /ui/
npm run build:ui

# OR build for root deployment
npm run build:root
```

## Configuration Methods

### Method 1: Environment Variables (Recommended)

Create a `.env.local` file:
```bash
# Deploy under /ui/ subpath
VITE_BASE_PATH=/ui/
```

Then build normally:
```bash
npm run build
```

### Method 2: Build Scripts

Use the pre-configured build scripts:
```bash
npm run build:ui      # Builds for /ui/ subpath
npm run build:root    # Builds for root path /
```

### Method 3: Command Line

```bash
# For /ui/ subpath
VITE_BASE_PATH=/ui/ npm run build

# For custom subpath
VITE_BASE_PATH=/coffee/admin/ npm run build
```

## Server Configuration

### ESP32/Arduino Web Server Example

```cpp
// Serve the UI under /ui/ path
server.serveStatic("/ui/", SPIFFS, "/ui/");

// Handle UI routes (React Router)
server.onNotFound([]() {
  String path = server.uri();

  // Check if it's a UI request
  if (path.startsWith("/ui/")) {
    // Remove /ui/ prefix for route checking
    String uiPath = path.substring(3);

    // Valid UI routes
    if (uiPath == "/" || uiPath == "" ||
        uiPath.startsWith("/settings") ||
        uiPath.startsWith("/system") ||
        uiPath.startsWith("/about")) {
      // Serve the SPA
      File file = SPIFFS.open("/ui/index.html", "r");
      server.streamFile(file, "text/html");
      file.close();
    } else {
      // Invalid UI route - return 404
      File file = SPIFFS.open("/ui/index.html", "r");
      server.streamFile(file, "text/html", 404);
      file.close();
    }
  } else {
    // Non-UI request - handle normally
    server.send(404, "text/plain", "Not found");
  }
});
```

### Nginx Configuration

```nginx
# Serve the UI under /ui/
location /ui/ {
    alias /path/to/ui/dist/;
    try_files $uri $uri/ /ui/index.html;
}

# API endpoints (if separate)
location /api/ {
    proxy_pass http://coffee-machine:8080;
}
```

### Apache .htaccess

```apache
# Place in the /ui/ directory
RewriteEngine On
RewriteCond %{REQUEST_FILENAME} !-f
RewriteCond %{REQUEST_FILENAME} !-d
RewriteRule . /ui/index.html [L]
```

## Testing

### 1. Test Different Subpaths

```bash
# Test /ui/ subpath
VITE_BASE_PATH=/ui/ npm run dev

# Test custom subpath
VITE_BASE_PATH=/coffee/admin/ npm run dev
```

### 2. Test Built Application

```bash
# Build and preview
npm run build:ui
npm run preview:ui

# Check URLs:
# ✅ http://localhost:4173/ui/
# ✅ http://localhost:4173/ui/settings/general
# ✅ http://localhost:4173/ui/system
# ❌ http://localhost:4173/ui/invalid-page (should show 404)
```

### 3. Verify Asset Paths

After building, check that all assets have the correct base path:
- CSS: `/ui/assets/index-[hash].css`
- JS: `/ui/assets/index-[hash].js`
- Images: `/ui/vite.svg`

## Troubleshooting

### Problem: White screen after deployment

**Solution**: Check that the server is configured to serve the SPA correctly for all UI routes.

### Problem: 404 for assets

**Solution**: Ensure the base path is set correctly and assets are served from the right location.

### Problem: API calls failing

**Solution**: Update API calls to use absolute URLs or configure a proxy:

```javascript
// In your fetch calls
const apiBase = import.meta.env.VITE_API_BASE_URL || '';
fetch(`${apiBase}/api/temperature`)
```

### Problem: Router navigation not working

**Solution**: Ensure BrowserRouter has the correct basename and server handles SPA routing.

## Current Configuration

The app is currently configured to:
- Default to root path (`/`) in development
- Deploy to `/ui/` in production (via `.env.production`)
- Support custom paths via `VITE_BASE_PATH` environment variable

## File Structure After Build

```
dist/
├── index.html          # Main HTML file
├── vite.svg           # Assets with correct base path
└── assets/
    ├── index-[hash].css
    └── index-[hash].js
```

Copy the entire `dist/` folder contents to your server's `/ui/` directory.
