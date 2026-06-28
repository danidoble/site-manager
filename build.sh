#!/usr/bin/env bash
# Buils both portable AppImage and .deb package for Local Site Manager (GUI entry point).

# build Appimage
./packaging/build-appimage.sh

# build .deb package
./packaging/build-deb.sh