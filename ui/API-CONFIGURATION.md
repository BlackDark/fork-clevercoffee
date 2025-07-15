# CleverCoffee UI with Environment-based API Configuration

The UI now supports configurable API endpoints through environment variables, making it easy to switch between development (mock server) and production (ESP32 hardware) environments.

## Environment Configurations

### 1. Mock Development Mode (`.env.mock`)
```bash
VITE_BASE_PATH=/
VITE_API_BASE_URL=http://localhost:3001
VITE_MOCK_MODE=true
```

**Usage:**
```bash
# Start mock server (in one terminal)
cd mock-server && npm start

# Start UI in mock mode (in another terminal)
cd ui && npm run dev:mock
```

### 2. Development Mode (`.env.development`)
```bash
VITE_BASE_PATH=/
# VITE_API_BASE_URL=  (uses relative URLs)
```

**Usage:**
```bash
cd ui && npm run dev
```

### 3. Production Mode (`.env.production`)
```bash
VITE_BASE_PATH=/ui/
# VITE_API_BASE_URL=/api/  (optional)
```

**Usage:**
```bash
cd ui && npm run build
```

## API Configuration Features

### Type-Safe API Calls
```typescript
import { apiFetch, apiJsonFetch } from "@/lib/api-config";
import type { TemperatureData, Parameter } from "@/types/api";

// Simple fetch with automatic URL construction
const response = await apiFetch("api/temperature");

// Typed JSON fetch
const data = await apiJsonFetch<TemperatureData>("api/temperature");

// POST with JSON
const result = await apiJsonFetch<ApiResponse>("api/steam", {
  method: "POST"
});
```

### Automatic URL Construction
The `createApiUrl()` function automatically handles:
- Mock server URLs: `http://localhost:3001/api/temperature`
- Relative URLs: `/api/temperature`
- Custom base URLs: `${VITE_API_BASE_URL}/api/temperature`

### Environment Detection
```typescript
import { isMockMode, apiBaseUrl } from "@/lib/api-config";

if (isMockMode) {
  console.log("Running in mock mode");
}
```

## Development Workflows

### 1. Frontend Development with Mock Server
```bash
# One command to start both servers
./dev-start.sh

# Or manually:
# Terminal 1: Mock server
cd mock-server && npm start

# Terminal 2: UI with proxy
cd ui && npm run dev:mock
```

### 2. Testing Against Real Hardware
```bash
# Configure environment for ESP32
cd ui
cp .env.development .env.local
# Edit .env.local to set VITE_API_BASE_URL=http://192.168.1.100

npm run dev
```

### 3. Production Build
```bash
cd ui
npm run build        # For root deployment
npm run build:ui     # For /ui/ subpath deployment
```

## Proxy Configuration

When in mock mode, Vite automatically proxies `/api/*` requests to the mock server:

```typescript
// vite.config.ts
if (mode === 'mock') {
  config.server = {
    proxy: {
      '/api': {
        target: 'http://localhost:3001',
        changeOrigin: true,
      },
    },
  };
}
```

## Type Definitions

All API types are centralized in `src/types/api.ts`:

```typescript
export interface TemperatureData {
  currentTemp: number;
  targetTemp: number;
  heaterPower: number;
}

export interface Parameter {
  type: number;
  name: string;
  displayName: string;
  value: number | string;
  // ... other properties
}

export interface ApiResponse {
  success: boolean;
  message?: string;
}
```

## Benefits

1. **Environment Flexibility**: Easy switching between mock and real hardware
2. **Type Safety**: Full TypeScript support for all API calls
3. **Error Handling**: Consistent error handling across all endpoints
4. **Development Speed**: No need for ESP32 hardware during UI development
5. **Testing**: Mock server enables automated testing and CI/CD
6. **Deployment Options**: Support for both root and subpath deployments

## Migration from Direct fetch()

Old code:
```typescript
const response = await fetch("/api/temperature");
const data = await response.json();
```

New code:
```typescript
const data = await apiJsonFetch<TemperatureData>("api/temperature");
```

The new approach provides:
- Automatic URL construction based on environment
- Type safety for responses
- Consistent error handling
- Mock mode logging
- Environment-specific configurations
