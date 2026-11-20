#!/bin/sh
# Writes target/frame_{plain,parchment}_{square,round}.png from the mod's own frame code.
cd "$(dirname "$0")/.." && cargo test --lib --offline -- --ignored frame_preview
