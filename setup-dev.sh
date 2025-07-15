#!/bin/bash

# CleverCoffee Development Setup Script
# This script helps set up the development environment with the mock server

echo "🚀 CleverCoffee Development Setup"
echo "=================================="

# Check if we're in the right directory
if [ ! -d "mock-server" ] || [ ! -d "ui" ]; then
    echo "❌ Error: Please run this script from the clevercoffee project root directory"
    echo "   Expected structure:"
    echo "   - clevercoffee/"
    echo "     - mock-server/"
    echo "     - ui/"
    exit 1
fi

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Check dependencies
echo "🔍 Checking dependencies..."

if ! command_exists node; then
    echo "❌ Node.js is not installed. Please install Node.js from https://nodejs.org/"
    exit 1
fi

if ! command_exists npm; then
    echo "❌ npm is not installed. Please install npm"
    exit 1
fi

echo "✅ Node.js and npm are installed"

# Install mock server dependencies if needed
echo "📦 Installing mock server dependencies..."
cd mock-server
if [ ! -d "node_modules" ]; then
    npm install
else
    echo "✅ Mock server dependencies already installed"
fi
cd ..

# Install UI dependencies if needed
echo "📦 Installing UI dependencies..."
cd ui
if [ ! -d "node_modules" ]; then
    npm install
else
    echo "✅ UI dependencies already installed"
fi

# Build UI if dist doesn't exist
if [ ! -d "dist" ]; then
    echo "🏗️  Building UI..."
    npm run build
else
    echo "✅ UI already built"
fi
cd ..

echo ""
echo "🎉 Setup complete!"
echo ""
echo "To start development:"
echo "1. Start the mock server:"
echo "   cd mock-server && npm start"
echo ""
echo "2. In another terminal, start the UI development server:"
echo "   cd ui && npm run dev"
echo ""
echo "3. Configure your UI to use the mock server at http://localhost:3001"
echo ""
echo "📚 For more information, see:"
echo "   - mock-server/README.md"
echo "   - ui/README.md"
