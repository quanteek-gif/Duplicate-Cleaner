# Duplicate Cleaner

Duplicate Cleaner is a desktop app for finding duplicate files and freeing up disk space. It compares file contents—not filenames—so it can find identical files even when their names differ.

Before moving files to the Recycle Bin, the app verifies them again and protects the last remaining copy in each duplicate group. Scan results and deletion operations are saved in history.

## Features

- Find exact duplicates using SHA-256.
- Find similar photos using perceptual hashing.
- Browse the largest files and visualize disk usage.
- Review scan and deletion history.
- Configure filters, performance, and the hash cache.
- Switch between English and Russian.

## Built with

Rust, Tauri 2, React, and TypeScript. File analysis runs locally.
