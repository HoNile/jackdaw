# Jackdaw

A 3D editor built for and with [Bevy](https://bevyengine.org/).

<img width="1917" height="1033" alt="image" src="https://github.com/user-attachments/assets/fa53542b-de0a-420b-bde0-bdcb180992a5" />

## Features

- **Brush-based geometry** — draw, edit, and CSG-combine convex brushes with vertex/edge/face/clip editing modes
- **Material system** — texture browser, material definitions with ORM auto-detection, per-face application
- **Terrain** — heightmap sculpting and texture painting
- **Scene serialization** — save/load scenes in the `.jsn` format with full asset references
- **Transform tools** — translate, rotate, scale with grid snapping and axis constraints
- **Undo/redo** — full command history
- **Extensible** — register custom components, add inspector panels, integrate with your game

## Usage

Add `jackdaw` to your project:

```sh
cargo add jackdaw
```

Then add the `EditorPlugin` to your app:

```rust
use bevy::prelude::*;
use jackdaw::EditorPlugin;

fn main() -> AppExit {
    App::new()
        .add_plugins((DefaultPlugins, EditorPlugin))
        ...
        .run()
}
```

See the [examples](examples/) for more advanced usage.

## Keyboard Shortcuts

### Navigation

| Key | Action |
|-----|--------|
| RMB + Drag | Look around |
| WASD | Move |
| Q / E | Move up / down |
| Shift | Double speed |
| Scroll | Dolly forward / back |
| F | Focus selected |

### Editing

| Key | Action |
|-----|--------|
| Esc / R / T | Translate / Rotate / Scale mode |
| B / C | Draw brush (add / cut) |
| 1-4 | Brush edit: Vertex / Edge / Face / Clip |
| Ctrl+D | Duplicate |
| Delete | Delete selected |
| Ctrl+Z / Ctrl+Shift+Z | Undo / Redo |
| Ctrl+S | Save scene |

For the full shortcuts reference, see the [book](https://jbuehler23.github.io/jackdaw/user-guide/keyboard-shortcuts.html).

## Documentation

- [Book](https://jbuehler23.github.io/jackdaw/) — user guide, developer guide, and reference
- [Examples](examples/) — runnable examples
- [API Docs](https://docs.rs/jackdaw) — rustdoc

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and PR guidelines.

## License

Dual-licensed under [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
