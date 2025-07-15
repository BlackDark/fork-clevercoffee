#!/bin/bash

# CleverCoffee Mock Server API Test
# Tests all API endpoints to ensure they work correctly

API_BASE="http://localhost:3001/api"

echo "🧪 Testing CleverCoffee Mock Server API"
echo "======================================="
echo "Server: $API_BASE"
echo ""

# Function to test an endpoint
test_endpoint() {
    local method=$1
    local endpoint=$2
    local expected_status=$3
    local description=$4

    echo -n "Testing $method $endpoint - $description... "

    if [ "$method" = "GET" ]; then
        response=$(curl -s -w "%{http_code}" -o /tmp/response.json "$API_BASE$endpoint")
    else
        response=$(curl -s -w "%{http_code}" -o /tmp/response.json -X "$method" "$API_BASE$endpoint")
    fi

    status_code="${response: -3}"

    if [ "$status_code" = "$expected_status" ]; then
        echo "✅ PASS ($status_code)"
    else
        echo "❌ FAIL (expected $expected_status, got $status_code)"
        if [ -f /tmp/response.json ]; then
            echo "   Response: $(cat /tmp/response.json)"
        fi
    fi
}

# Check if server is running
echo "Checking if mock server is running..."
if ! curl -s "$API_BASE/temperature" > /dev/null 2>&1; then
    echo "❌ Mock server is not running on localhost:3001"
    echo "Please start it first with: cd mock-server && npm start"
    exit 1
fi
echo "✅ Mock server is running"
echo ""

# Test all endpoints
echo "Testing API endpoints:"
echo "----------------------"

# Machine control endpoints
test_endpoint "POST" "/steam" "200" "Toggle steam mode"
test_endpoint "POST" "/pid" "200" "Toggle PID control"
test_endpoint "POST" "/backflush" "200" "Toggle backflush mode"
test_endpoint "POST" "/scale/tare" "200" "Tare scale"
test_endpoint "POST" "/scale/calibration" "200" "Scale calibration"

# Data endpoints
test_endpoint "GET" "/parameters" "200" "Get all parameters"
test_endpoint "GET" "/parameters?filter=hardware" "200" "Get hardware parameters"
test_endpoint "GET" "/parameter-help?param=pid.enabled" "200" "Get parameter help"
test_endpoint "GET" "/temperature" "200" "Get current temperature"
test_endpoint "GET" "/temperatures" "200" "Get temperatures (alias)"
test_endpoint "GET" "/history" "200" "Get temperature history"

# System endpoints
test_endpoint "POST" "/wifi-reset" "200" "WiFi reset"
test_endpoint "GET" "/config/download" "200" "Download configuration"
test_endpoint "POST" "/restart" "200" "Restart device"
test_endpoint "POST" "/factory-reset" "200" "Factory reset"

# Error cases
test_endpoint "GET" "/nonexistent" "404" "Non-existent endpoint"
test_endpoint "GET" "/parameter-help" "422" "Missing parameter"

echo ""
echo "🎉 API testing complete!"
echo ""
echo "Sample responses:"
echo "=================="
echo "Temperature data:"
curl -s "$API_BASE/temperature" | jq . 2>/dev/null || curl -s "$API_BASE/temperature"
echo ""
echo ""
echo "Parameters (first 2):"
curl -s "$API_BASE/parameters" | jq '.[0:2]' 2>/dev/null || curl -s "$API_BASE/parameters" | head -c 200
echo ""

# Cleanup
rm -f /tmp/response.json
