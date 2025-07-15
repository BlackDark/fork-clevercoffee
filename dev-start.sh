#!/bin/bash

# CleverCoffee Development Launcher
# Starts both the mock server and UI development server

echo "🚀 Starting CleverCoffee Development Environment"
echo "================================================"

# Check if we're in the right directory
if [ ! -d "mock-server" ] || [ ! -d "ui" ]; then
    echo "❌ Error: Please run this script from the clevercoffee project root directory"
    exit 1
fi

# Function to cleanup on exit
cleanup() {
    echo ""
    echo "🛑 Shutting down development servers..."
    # Kill all background jobs
    jobs -p | xargs -r kill
    exit 0
}

# Set up signal handling
trap cleanup SIGINT SIGTERM

echo "📡 Starting mock server on http://localhost:3001..."
cd mock-server
npm start &
MOCK_PID=$!
cd ..

# Wait a moment for the mock server to start
sleep 2

echo "🌐 Starting UI development server..."
cd ui
npm run dev:mock &
UI_PID=$!
cd ..

echo ""
echo "✅ Development environment is ready!"
echo ""
echo "🔗 Available endpoints:"
echo "   Mock API:  http://localhost:3001/api/*"
echo "   UI Dev:    http://localhost:5173/"
echo ""
echo "Press Ctrl+C to stop all servers"
echo ""

# Wait for background processes
wait
