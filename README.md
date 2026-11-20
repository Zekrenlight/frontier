# Frontier

A minimap, a world map of everywhere you have been, and waypoints, for the Sunrise client
(game 26.3, mod API 0.2). Client side only; no sounds.

<img width="353" height="401" alt="image" src="https://github.com/user-attachments/assets/d1f7bad8-9c8e-4182-8097-9ba1661b261d" />
<img width="2554" height="1361" alt="image" src="https://github.com/user-attachments/assets/fe8a9b12-3008-4797-9521-f620bcee8e82" />


## Minimap

- Round (the default) or square, in any corner, 64–192 GUI pixels wide (64 at first), five zoom
  levels (keys `=` / `-`; one block a map pixel at first, so the map shows about 64 blocks across).
- North up, or turning with you; an arrow for you and N/E/S/W letters round the edge.
- Your coordinates and biome name under the map (each can be turned off).
- Dims at night with the world's light (toggle). Hill shading (toggle).
- Cave mode (toggle): underground, or under any roof, glass domes and glass roofs included, the
  map shows the floor around you (within 96 blocks) instead of the surface; the room under a
  glass ceiling shows through it. It starts once most of the ground round you has been under
  a roof for a few ticks and ends once it steadily isn't, so a one-block overhang, a dome's
  seam or a tree's leaves never flick it; the two pictures crossfade over about a third of a
  second. Any cover overhead pre-warms the cave picture, and a column not yet read keeps the
  surface during the fade, so walking down a long staircase never flashes or pops. In the
  Nether the map always reads below the roof.
- Smooth: the picture is redrawn from the camera's place every frame it moves, and the dots,
  waypoints, arrow and compass are placed by that same view, so they glide with the map.
  Reading the world is spread over ticks with a time limit, so new chunks or the first moments
  in a cave never stall a frame, and the cave picture is kept when you step out, so going back
  in is instant.
- Frames: **Plain** (a dark rim, the default), **Parchment** (frayed paper), or **None**. Frames are drawn by
  the mod's own code at start-up; there are no image files.
- Block colours come from each block's own top texture (averaged), tinted like the game tints
  grass, leaves and water, then shaded by Frontier's own hill and water-depth shading.

## Creatures, things and blocks on the maps

- Dots for what is near you: **red** for monsters, **green** for animals and other peaceful
  creatures, **yellow** for villagers and wandering traders, **white** for other players.
  Each kind can be turned off, or all of them at once.
- **Faces** (the default; or plain dots), shown while names show (holding `Tab` at first):
  each creature's face, made at run time from its own model (its head seen straight on, or
  from the side for horses, camels, goats and fish) and the texture the game draws it with
  (its variant, your resource packs, and their custom entity models), on a rim of its kind's colour. Players show their
  skin's face and hat layer, drawn as chat heads are. Anything without a face keeps a dot.
- **Creatures from other mods**: a creature another mod draws with its own renderer (on top of
  a vanilla type it copies, such as a wandering trader) shows the marker image its mod or a
  resource pack gives (see *Map markers* below), else a plain dot, never the copied type's face.
- **Dropped items** (toggle, on at first): items lying on the ground, as small **black** dots on
  a light rim with a faint dark halo, clear on dark rock and on snow alike. While names show
  (holding `Tab` at first) each one is drawn as **its item's own icon** (as the game draws it in
  a slot, with your resource packs, on no backing) with **its real name** beside it: the custom name of a
  renamed item, else the item's name in your language, with the count (`Cobblestone ×16`).
  Pointing at one on the world map names it too.
- **Things** (toggle, off at first): item frames, armour stands, minecarts, boats, paintings
  and cushions, as small **purple** diamonds.
- **Storage** (toggle, off at first): chests, barrels, shulker boxes, furnaces, hoppers and the like, as
  **amber** squares. **Decorations** (toggle, off at first): signs, banners, beds, heads,
  lecterns, campfires and the like, as **teal** squares.
- Names and faces: while the player list key (`Tab`) is held (the default), always, or never.
  Then a creature with a face shows its face, and anything without one its name (its own,
  else its type's). Players keep their name beside their head (*Always Show Player Names*,
  on at first). Holding `Tab` on the world map doesn't move between its buttons.
- Range: as far as the minimap shows (the default), or a set distance up to 256 blocks.
- Height fade (toggle): dots far above or below you fade, with a tick on top (above) or
  underneath (below).
- Only what is on the layer you are looking at is shown (other players always are, so family
  on a server stay visible at any height). In cave mode, that is within about 5 blocks of your
  height (or, on a cave layer of the world map, of that layer's floor). On the surface, mobs,
  items and things under the ground are hidden; ones on hills and in valleys still show.
- Everything the minimap shows is on the world map too, with the same switches (toggle).
- Invisible creatures are left out.
- The minimap steps aside while the F3 debug screen is up (toggle).

## World map (`M`)

- Everything you have seen, saved per server or single-player world and per dimension.
- **Surface** and **cave layers**: underground, the map is cut in 16-block layers of height
  (`Cave · Y 48–63`), each saved on its own, so a base at Y 10, a cave at Y -40 and the inside
  of a dome at Y 70 never write over one another or the surface. The map opens in the view you
  are in: in cave mode (underground, or under a roof or dome) straight onto **your own layer**,
  cut just over your head, so you see the room you are standing in, not the roof over it, and it
  follows you up and down. A cave layer shown is read live across **every chunk the game has
  loaded** round the view, a few milliseconds a frame, so a whole base fills in at once.
- The top bar names the dimension (`Overworld`); the **?** at its right end lists the map's keys
  while the pointer is on it. Nothing else is written over the map, so the bar and the controls
  never overlap or run off the screen, down to 320 GUI pixels wide.
- Top left: the **Surface | Cave** switch (or `C`); in a cave view, **▼ / ▲** (or `[` / `]`,
  Page Down / Page Up) step a layer down or up, each layer kept as you leave it. Changing the
  view redraws the map at once. The Y under the cursor and a right-click waypoint's height come
  from the layer shown; dots, waypoints, the menu, the compass and panning and zooming work the
  same in every view.
- Drag to move, scroll to zoom (1/8 to 32 blocks per pixel), arrow keys / WASD to pan,
  **Centre** to come back. Coordinates under the cursor; your arrow, your waypoints and the
  minimap's dots on it. Point at a dot to see its name.
- Right-click for a menu (toggle): on a waypoint, *Edit waypoint* and *Remove waypoint…* (asks
  first); anywhere else, *Add a waypoint here*; then *Copy coordinates*, *Share in chat* (typed
  into the chat box for you to send or not) and *Frontier settings*. Its top line is the
  waypoint's name, or the **biome** there (in your language; Frontier remembers the biome of
  each chunk you explore, so far-off spots are named too), then the coordinates; a spot never
  explored shows the coordinates alone.
- **Teleport here** (toggle, on at first): in Creative or Spectator, when the server lets you
  use `/tp` (permission level 2), the menu also offers *Teleport here*, on a waypoint or
  anywhere. It sends `/tp @s <x> <y> <z>` to the middle of the block: onto the ground there
  plus one on the surface view, onto the floor of the cave layer shown in a cave view, to the
  waypoint's own height on a waypoint, and at your own height (`~`) where the ground there
  hasn't been read. The server has the last word, as for any command.
- A small compass in the bottom right corner (toggle).

## Sharp maps

On by default: the minimap picture has twice the pixels (every block a whole number of them)
and the world map two pixels per GUI pixel, drawn without blending blocks into each other.
Off goes back to the lighter first look.

## Waypoints

- `B` makes one where you stand (name and colour, 12 colours); `J` lists them: show/hide, edit,
  delete (asks first), new.
- Shown on the minimap (far ones on its edge) and on the world map; in the world as coloured
  beams and floating name + distance labels, seen through walls (each a toggle, with a range).
- A **Last Death** waypoint (toggle) appears where you died and clears itself when you get back.
- Kept per server or world and per dimension.

## Keys (all can be changed in Options > Controls, category *Frontier*)

| Key | Action |
| --- | --- |
| `M` | Open the world map |
| `B` | New waypoint |
| `J` | Waypoint list |
| `K` | Frontier settings |
| `=` / `-` | Minimap zoom in / out |
| (unbound) | Show / hide the minimap |

## Settings

Options > Mods... > Frontier (or `K`, or `/frontier settings`): minimap on/off, shape, corner,
size, zoom, turn with player, frame, compass letters, coordinates, biome, day and night light,
hill shading, cave mode, waypoints on minimap, beacons, labels, beacon and label range, death
waypoint, show mobs, show hostile, show passive, show players, show villagers, mob range,
height fade, everything on the world map, creature icons (dots or faces), names and faces,
always show player names, show dropped items, show things,
show storage, show decorations, sharp maps, hide under F3, world map compass, world map menu,
teleport from the map.

## Map markers for other mods and resource packs

Frontier looks for a marker image for every creature type before it makes a face itself:

```
assets/<namespace>/textures/frontier/markers/<path>.png     for entity type <namespace>:<path>
```

- A mod ships it in its own assets: `vulture:vulture` is
  `assets/vulture/textures/frontier/markers/vulture.png`.
- A resource pack can add one for any type, vanilla too
  (`assets/minecraft/textures/frontier/markers/cow.png`), or replace a mod's; the highest pack
  wins, as for any texture.
- Any size and shape with transparency; it is shown 8×8 (bigger images are averaged down, so
  8×8 or 16×16 drawn for that size looks best), on the rim of the creature's kind colour,
  while faces show (holding `Tab` at first).
- A creature type a mod draws with its own renderer and no marker shows a plain dot.
  Nothing has to be registered in code: dropping the file in is enough, and Frontier picks it
  up again when resource packs reload.

## Command

`/frontier` (menu), `/frontier map`, `/frontier waypoints`, `/frontier settings`,
`/frontier add [name]` (a waypoint where you stand).

## Files

Maps: `maps/<world>/<dimension>/r.<x>.<z>.bin` (256×256 columns per file, run-length encoded);
each cave layer in `maps/<world>/<dimension>/caves/y_<layer>/r.<x>.<z>.bin`, the same way
(`y_4` is Y 64–79, `y_-3` is Y -48 to -33).
Biomes: `maps/<world>/<dimension>/b.<x>.<z>.txt`, one per 256×256-block region: the biome of
each chunk explored (for the world map menu's top line).
Waypoints: `waypoints/<world>/<dimension>.txt`, one per line: `x y z colour shown kind name`.
Both are in the mod's data folder.

## Look and settings

Frontier uses the shared Sunrise dark look: a `#0F1118` card with a 2 px accent edge, tabs and
charcoal chips. The accent is Sunrise orange `#F9801D` at first; change it with **Accent colour**
in the settings card (eight presets, or Reset).
The world map's right-click menu uses the same card and accent.

The settings card opens from SunnyMods' **Settings** button, from Sunrise's own Mods list, or
from the mod's settings key. **Key binds › Rebind** switches to Sunrise's classic page to change keys.

## Compatibility

| Piece | Pin |
| --- | --- |
| Minecraft Java | 26.3 |
| Sunrise API | 0.2 |
| Mod SDK (`[native].sunrise`) | `94256510f74e6843754e0a93fd9e4da7e45b1056` (`9425651`, 19 Nov 2026) |
| Mixins (`[native].mixins`) | `e59da9094f0c2fdda6c627fe9eff22dec6a906f0` |

Frontier works on its own and alongside the other Sunrise mods by Zekrenlight. Its settings-card
hooks act only on its own Options page, so several mods can be installed together without any
conflicting replacements. A mod with native hooks works only on the exact SDK build it was made for.

## Building

```sh
cargo sunrise-mod           # fetches the pinned SDK into .sunrise/ (gitignored)
cargo sunrise-mod check     # type-checks against the pinned SDK
cargo test --lib            # unit tests, no game needed
cargo sunrise-mod build     # target/sunrise-mod/frontier.smod
```

```sh
tools/preview_frames.sh            # target/frame_*.png
python3 tools/make_icon.py icon.png
```

To move to a newer SDK, run `cargo sunrise-mod update --sunrise <build>` (it rewrites the
`sunrise` and `mixins` lines in `sunrise-mod.toml`), then check and rebuild.

Install the `.smod` with sunrise-launcher (**Add from file**), or point **Add from GitHub** at
this repository. Native mods run with full access to your computer; read the source first.

## License

MIT © 2026 Zekrenlight. See [LICENSE](LICENSE).
