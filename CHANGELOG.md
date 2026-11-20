# Changelog

## 1.0.0 (2026-11-19)

First release of Frontier, an original Sunrise native mod by Zekrenlight (MIT).

- A minimap and world map with waypoints, a creature radar, storage and decoration markers, sharp maps and a death waypoint.
- The world map's right-click menu uses the shared dark card and accent.
- Cave mode starts under any roof, glass domes and glass roofs included (leaves don't count), and shows the room under a glass ceiling; it starts and ends only on a steady reading, so overhangs and dome seams never flick it, and the surface and cave pictures crossfade over about 320 ms (eased). Any overhead cover pre-warms the cave picture, walking down stairs re-sweeps the floor in place instead of wiping it, and a column not yet read keeps the surface pixel during the fade, so a long staircase never flashes or pops.
- Smooth minimap: redrawn from the camera's place each frame it moves, with dots, waypoints, the arrow and the compass placed by that same view; world reading is time-limited per tick, and the cave picture is kept when you step out.
- Creatures, dropped items and things on both maps stay on the layer you are looking at (within about 5 blocks). Other players are always shown, at any height. On the surface, anything under the ground is hidden; mobs on hills and in valleys still show. The same filter applies to hover and Tab names and icons.
- The world map has layered cave views: one per 16 blocks of height (`Cave · Y 48–63`), each saved on its own under `caves/y_<layer>/`, so caves, bases and dome insides at different heights never overwrite each other or the surface. Underground (or under a dome) the map opens straight onto your own layer, cut just over your head, and follows you; a cave layer shown is read live across every loaded chunk round the view within a per-frame time limit. **Surface | Cave** (or `C`), **▼ / ▲** (or `[` / `]`, Page Down / Page Up) change the view, and a change redraws at once.
- Dropped items show as black dots (on at first, their own toggle); holding `Tab` shows each one's item icon and real name (custom name, else the item's own name, with the count). Other things (boats, minecarts, armour stands and the like) are purple and off at first.
- Built for Minecraft Java 26.3, Sunrise API 0.2, Mod SDK `9425651` (`94256510f74e6843754e0a93fd9e4da7e45b1056`), mixins `e59da909`.
- Settings open in the shared Sunrise dark card (`#0F1118`, accent edge, charcoal chips) from SunnyMods' Settings button, Sunrise's Mods list, or the mod's settings key; its hooks act only on the mod's own Options page, so it works alone or with the other Sunrise mods.
- An **Accent colour** setting (Sunrise orange `#F9801D` at first) with eight presets and Reset.
