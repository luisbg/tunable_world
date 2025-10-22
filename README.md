# TuneWorld — Bevy Post-Processing Playground & Scene Editor

An experimental scene editor and post-processing playground built with <a href="https://bevyengine.org/">Bevy</a>.</b><br>
Real-time tweakable shaders, atmospheric lighting, and Egui-driven visual editing.
</p>

![A screenshot of TuneWorld in action](https://github.com/luisbg/tunable_world/raw/main/assets/screenshots/0.png)

<p align="center">
  <a href="https://bevyengine.org/">Bevy 0.16</a> •
  <a href="https://wgpu.rs/">wgpu</a> •
  <a href="https://github.com/emilk/egui">Egui</a> •
  MIT License
</p>

---

## Features

- **Real-time post-processing pipeline**
  - Bloom, Depth of Field, LUT, Tone Mapping, Gradient Tint, and more
  - Enable/disable effects individually, tweak parameters interactively

- **Scene editing tools**
  - Add, delete, move, resize, rotate scene objects
  - Reload and experiment with world setups dynamically

- **Shader experimentation**
  - Add your own WGSL post-process passes (CRT, gradient tint, LUT, etc.)
  - Modular plugin architecture for rapid iteration

- **Egui-based control panels**
  - “Effect Settings” window
  - Live updates while editing values

---

## How It Works

TuneWorld is a **Bevy app** that:
1. Loads a 3D scene and camera setup.
2. Chains post-processing passes through custom render nodes.
3. Exposes shader parameters via **Egui** sliders and checkboxes.
4. Updates GPU uniforms in real time.

### Project Structure
```plaintext
src/
├── main.rs                # Entry point
├── camera.rs              # Camera setup and control code
├── inspector.rs           # Code for the UI to change or add scene objects
├── post/                  # Post-processing shaders & render nodes
├── ui/                    # Egui control panels
assets/
└── shaders/               # WGSL shader files
└── luts/                  # Example color lookup table files
```

---

## Controls & Shortcuts

| Key | Action |
|-----|--------|
| ** 1 / 2 / 3 / 4** | Move camera to N/S/W/E coordinates |
| **Q / E** | Move camera to prev/next coordinate |
| **A / D** | Rotate camera smoothly |
| Spacebar | Show and hides the Inspector UI |

Click on any object to select it (and have the Inspector UI appear).
In the Inspector UI scenes can be saved and loaded from JSON files.

---

## Egui Panels

### Effect Settings

**Sections:**
- **Depth of Field** – Adjust focal distance, aperture (f-stops), and bokeh toggle  
- **Outlines** - Set width
- **Chromatic Aberration** - Adjust intensity
- **CRT** - Tweak intensity, scanline frequency, and line intensity
- **Gradient Tint** – Blend two colors (top-right ↔ bottom-left)  
- **LUT** – Select a color lookup table PNG file and apply
- **Bloom** – Enable/Disable bloom (intensity slider WIP)  
- **Tone Mapping** – Enable/Disable tone mapping 
- **Distance Fog** – Enable/Disable fog, (adjust falloff and distance WIP)

---

## 🛠️ Installation

### Prerequisites
- Rust **1.80+**
- **Bevy 0.16**
- GPU supporting **Vulkan**, **Metal**, **DX12**, or **WebGPU**

### Run Locally
```bash
git clone https://github.com/luisbg/tunable_world.git
cd tune_world
cargo run
```

---

## Adding New Effects

To create your own post-process shader:

1. Add your WGSL file to `assets/shaders/`
2. Create a Rust module in `src/post/your_effect.rs`
3. Register it as a plugin in `main.rs`
4. Add tweakable parameters in `ui/effect_settings.rs`

Each effect runs as an independent plugin with its own uniforms and render node logic.

---

## Roadmap

- [ ] Add sliders for Bloom and Tone Mapping parameters  
- [ ] Tilt/move camera up and down for better view
- [ ] Edit emissive material property in the material editor inside the Inspector
- [ ] Scene save/load as serialized binary files  
- [ ] Preset gallery (CRT, chromatic aberration, vignette)
- [ ] Select object groups in the Inspector (to move or copy in bulk)
- [ ] Add more shaders/effects
- [ ] Exit with Escape key
- [ ] Add alpha values in the Gradient Tint effect
- [ ] Make material on sides of a Cuboid different than the top face
- [ ] Add cylinder objects

---

## Contributing

Pull requests and shader contributions are welcome!  
Follow Bevy ECS best practices and keep modules isolated for clarity.

---

## License

**MIT License © 2025 — Luis de Bethencourt**  
See [`LICENSE`](LICENSE) for full terms.
