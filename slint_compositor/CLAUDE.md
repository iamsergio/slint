# slint-compositor

A crate for writing Wayland compositors in the Slint language, like QtWaylandCompositor does for QML.
Smithay provides the Wayland protocols, input, and DRM.
The first target is kiosk use, with layer-shell later.
The API is Rust only.

Track progress in [TODO.md](TODO.md) and tick boxes as items land.

This project uses git-loom, see the git loom skill and the committer skill.

## Paths

- Slint checkout: `/pub_data/sources/slint` (this directory lives inside it but is a separate project).
- Smithay: `/pub_data/sources/smithay` (0.7, calloop 0.14).
  `smithay/smallvil` is the protocol template, `smithay/anvil` the full reference.
- QtWaylandCompositor: `/pub_data/sources/qt/qt6_ci/qtwayland`.
  API in `src/compositor/compositor_api` and `src/compositor/extensions`, examples in `examples/wayland`.

Depend on Slint and smithay by path, so Slint changes can be tested here before they're upstream.

## Plan

### Clients

A Slint app on Wayland is already a client through the winit backend.
Don't build a client platform until P4, and then only for layer-shell and custom extensions.

### Packaging

The crate is a Slint library: `build.rs` uses `CompilerConfiguration::as_library(...).rust_module(...)`.
Apps import the `.slint` types and get the global with `ui.global::<slint_compositor::WaylandCompositor>()`.
See `/pub_data/sources/slint/tests/manual/module-builds` for a working example.

The `.slint` side provides:

- `global WaylandCompositor`: models of toplevels, creation and destruction callbacks, functions like `close` and `activate`.
- `ShellSurfaceItem`: a pure `.slint` component built from `Image`, `TouchArea`, and `FocusScope`.
  Its callbacks go through the global to Rust, which talks to the smithay seat.
- Window management, layout, and decorations stay in the app's `.slint` code.

### Platform

P0 runs on Slint's stock winit backend, with a helper thread that polls the Wayland fds and calls `invoke_from_event_loop`.
From P1 on, the crate ships its own `slint::platform::Platform` on smithay backends, because the stock backends don't fit:

- `KeyEvent` carries only text, but `wl_keyboard` needs evdev keycodes.
- Slint's event loop doesn't accept foreign fds.
- The stock backends keep their GL context private, so client textures can't be shared without copies.
- A kiosk on DRM needs control of outputs, the session, and planes.

The P1 platform uses calloop and renders with the public `slint::platform::femtovg_renderer::FemtoVGRenderer`.
An `OpenGLInterface` implementation wraps smithay's EGL surface.
A smithay `GlesRenderer` on a shared EGL context imports client buffers.
`BorrowedOpenGLTextureBuilder` turns the textures into `slint::Image`s.
Skia isn't an option on a custom platform, since only its WGPU renderer is public.

### Phases

- P0: spike on stock winit, shm copied into `SharedPixelBuffer`, pointer only.
- P1: own platform, nested through `smithay::backend::winit`, dmabuf, keyboard, popups, cursor.
- P2: DRM/KMS standalone kiosk, libseat, libinput, touch, one Slint window per output.
- P3: decorations, layer-shell, data device, input method, viewporter, fractional scale.
- P4: client backend for layer-shell, damage tracking, direct scanout.

Don't start a phase before the previous one's boxes are ticked, unless the user says so.

## Conventions

- Follow `/pub_data/sources/slint/docs/internal/writing-style-guide.md` for comments, Markdown, and commit messages.
- Match smithay's patterns (`delegate_*!` macros, handler traits) in protocol code.
- Keep all Wayland state on the UI thread.
- Prefer CSS naming for new `.slint` properties, as in Slint itself.
- Name `.slint` types after their QtWaylandCompositor counterparts where the concept matches, to ease porting.

## Testing

- Run the example nested in a Wayland session, then start clients with the printed `WAYLAND_DISPLAY`.
- shm clients for P0: `weston-simple-shm`, `weston-terminal`, `foot`.
- Slint clients in P0: `SLINT_BACKEND=winit-software`, since P0 has no dmabuf.
- EGL clients in P0: `LIBGL_ALWAYS_SOFTWARE=1`, which makes Mesa use `wl_shm`.
- Verify a feature with a real client before ticking its box in `TODO.md`.
