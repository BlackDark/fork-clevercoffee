# 404 Status Code Configuration

## Current Implementation

The React application now includes a proper 404 page (`/src/pages/NotFound.tsx`) that:
- Shows when users navigate to non-existent routes
- Sets appropriate document title and meta tags
- Provides navigation options to return to valid pages
- Logs 404 occurrences for debugging

## HTTP Status Code Issue

**Important**: The current implementation only shows a 404 page in the UI, but the HTTP status code is still 200 (OK) because this is a Single Page Application (SPA).

## Server-Side Solutions

To return proper HTTP 404 status codes, the embedded web server needs to be configured. Here are the recommended approaches:

### Option 1: Embedded Server Configuration (Recommended)

If the coffee machine uses an embedded HTTP server (like ESP32 WebServer, Arduino WebServer, or similar), modify the server code to:

```cpp
// Example for ESP32/Arduino
server.onNotFound([]() {
  // Check if it's an API request
  if (server.uri().startsWith("/api/") ||
      server.uri().startsWith("/parameters") ||
      server.uri().startsWith("/temperature")) {
    server.send(404, "application/json", "{\"error\":\"Endpoint not found\"}");
    return;
  }

  // For UI routes, serve the SPA with 404 status
  File file = SPIFFS.open("/index.html", "r");
  if (file) {
    server.streamFile(file, "text/html", 404); // Note the 404 status code
    file.close();
  } else {
    server.send(404, "text/plain", "File not found");
  }
});
```

### Option 2: Check Route on Server Side

Add a route handler that checks if the requested path is a valid UI route:

```cpp
// Valid UI routes
const char* validRoutes[] = {"/", "/settings/", "/system", "/about"};

bool isValidUIRoute(String path) {
  for (const char* route : validRoutes) {
    if (path.startsWith(route)) return true;
  }
  return false;
}

server.onNotFound([]() {
  String path = server.uri();

  if (isValidUIRoute(path)) {
    // Serve SPA with 200 status for valid UI routes
    File file = SPIFFS.open("/index.html", "r");
    server.streamFile(file, "text/html");
    file.close();
  } else {
    // Return 404 for invalid routes
    File file = SPIFFS.open("/index.html", "r");
    server.streamFile(file, "text/html", 404);
    file.close();
  }
});
```

### Option 3: Client-Side Status Simulation

For development/testing, you can simulate the behavior by checking the current URL:

```javascript
// In NotFound.tsx useEffect
useEffect(() => {
  // Simulate 404 status for debugging
  if (typeof window !== 'undefined' && window.location.pathname !== '/') {
    console.error(`404 Status: ${window.location.pathname} not found`);

    // You could also send analytics or logging data here
    // analytics.track('404_error', { path: window.location.pathname });
  }
}, []);
```

## Testing 404 Behavior

To test the 404 behavior:

1. **Development**: Navigate to `http://localhost:3000/nonexistent-page`
2. **Production**: Navigate to any invalid route on the coffee machine's web interface
3. **API Testing**: Try accessing invalid API endpoints like `/api/nonexistent`

## Production Deployment

When deploying the built application:

1. Build the project: `npm run build`
2. Copy the `dist/` folder contents to the coffee machine's web server storage
3. Configure the embedded server to handle 404s as described above
4. Test both UI and API 404 responses

## Browser Network Tab

To verify the HTTP status codes:
1. Open browser Developer Tools → Network tab
2. Navigate to a non-existent page
3. Check the status code of the main document request
4. Should show 404 instead of 200 for invalid routes
