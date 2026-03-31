#!/bin/bash
set -e

# JAVA_HOME must point to JDK 21+ (JDK 25 is incompatible with Gradle 8.x)
export JAVA_HOME="/home/med/.local/java/jdk-21.0.10+7"

# Start Waydroid if not running
if ! waydroid status | grep -q "RUNNING"; then
    echo "Starting Waydroid..."
    waydroid session start
    sleep 5
fi

# Connect ADB
echo "Connecting ADB..."
waydroid adb connect
sleep 2

# Verify connection
adb devices

# Build APK (x86_64 for Waydroid)
echo "Building APK..."
cd "$(dirname "$0")"
cargo tauri android build --apk --debug --target x86_64

# Install APK
echo "Installing APK..."
adb install -r src-tauri/gen/android/app/build/outputs/apk/universal/debug/app-universal-debug.apk

echo "Deploy complete!"
