# CleverCoffee Mock Server

A mock server that implements all CleverCoffee API endpoints for development and testing purposes.

## Features

- 🚀 Complete API endpoint coverage matching the ESP32 implementation
- 📊 Realistic mock data with dynamic temperature variations
- 🔄 Stateful parameter management
- 📁 File upload/download simulation
- 🌐 CORS enabled for cross-origin requests
- 📱 Optional UI serving from built React app

## Quick Start

```bash
# Install dependencies
npm install

# Start the server
npm start

# Or start with auto-reload during development
npm run dev
```

The server will start on `http://localhost:3001`

## API Endpoints

### Machine Control
- `POST /api/steam` - Toggle steam mode
- `POST /api/pid` - Toggle PID control
- `POST /api/backflush` - Toggle backflush mode
- `POST /api/scale/tare` - Tare scale
- `POST /api/scale/calibration` - Start scale calibration

### Data Endpoints
- `GET /api/parameters` - Get parameters (supports `?filter=hardware|behavior|other`)
- `POST /api/parameters` - Update parameters (form data)
- `GET /api/parameter-help?param=name` - Get parameter help text
- `GET /api/temperature` - Get current temperature data
- `GET /api/history` - Get temperature history data

### System Endpoints
- `POST /api/wifi-reset` - Reset WiFi settings
- `GET /api/config/download` - Download configuration file
- `POST /api/config/upload` - Upload configuration file
- `POST /api/restart` - Restart device
- `POST /api/factory-reset` - Factory reset

## Usage with UI Development

1. **Build the UI first:**
   ```bash
   cd ../ui
   npm run build
   ```

2. **Start the mock server:**
   ```bash
   cd ../mock-server
   npm start
   ```

3. **Configure your UI to use the mock server:**
   Update your UI's environment or proxy settings to point to `http://localhost:3001`

## Environment Variables

- `PORT` - Server port (default: 3001)

## Mock Data

The server maintains realistic mock data including:

- **Parameters**: PID settings, temperature controls, system configuration
- **Temperature Data**: Dynamic temperature readings with realistic variations
- **History Data**: Generated time series data for charts
- **State Management**: Persistent toggle states during server session

## Authentication

For simplicity, the mock server bypasses authentication. In a real ESP32 environment, authentication would be handled according to the `system.auth.enabled` configuration.

## File Uploads

Configuration file uploads are handled with `multer` and temporary file storage. Files are validated and cleaned up after processing.

## Development

The server uses modern ES modules and can be extended easily:

```javascript
// Add new endpoints
app.post('/api/my-endpoint', simulateAuth, (req, res) => {
  res.json({ success: true, message: "Custom endpoint" });
});
```

## Logging

All API requests are logged to the console for debugging:

```
Steam mode toggled: ON
Parameters requested (filter: hardware), returning 3 parameters
Config download requested
```

## Compatibility

This mock server is designed to be 100% compatible with the ESP32 API implementation in `embeddedWebserver.h`, ensuring seamless switching between mock and real hardware during development.
