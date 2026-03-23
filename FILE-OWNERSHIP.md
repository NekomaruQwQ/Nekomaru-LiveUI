# FILE-OWNERSHIP: Per-file ownership tracking

**agent** = Claude manages.
**human** = Nekomaru hand-crafts.

```bash
# config files at repo root
.gitignore                                  human
.justfile                                   human
.mod.nu                                     human
biome.json                                  human
Cargo.toml                                  human
FILE-OWNERSHIP.md                           human

# config files (crates + packages)
live-app/Cargo.toml                         human
live-capture/Cargo.toml                     human
live-kpm/Cargo.toml                         human
live-protocol/Cargo.toml                    human
live-ws/Cargo.toml                          human
crates/enumerate-windows/Cargo.toml         human
crates/set-dpi-awareness/Cargo.toml         human
crates/job-object/Cargo.toml                human
server/package.json                         human
server/tsconfig.json                        human
frontend/package.json                       human
frontend/tsconfig.json                      human
frontend/vite.config.ts                     human

# docs/
ARCHIVE-0-Prototype.md                      agent
ARCHIVE-1-StreamID.md                       agent
M4-DESIGN.md                               agent
PLAN-UI-AudioMeter.md                       agent
PLAN-UI-KPMMeter.md                         agent
README.md                                   agent
README-Audio.md                             agent

# live-protocol/
src/lib.rs                                  agent
src/avcc.rs                                 agent
src/video.rs                                agent

# live-capture/
src/main.rs                                 agent
src/lib.rs                                  agent
src/capture.rs                              agent
src/converter.rs                            agent
src/d3d11.rs                                agent
src/encoder.rs                              agent
src/encoder/debug.rs                        agent
src/encoder/helper.rs                       agent
src/resample.hlsl                           human
src/resample.rs                             human
src/selector/mod.rs                         agent
src/selector/config.rs                      agent

# live-ws/
src/main.rs                                 agent

# live-kpm/
src/main.rs                                 agent
src/hook.rs                                 agent
src/calculator.rs                           agent
src/message_pump.rs                         agent

# live-app/
src/main.rs                                 human

# crates/enumerate-windows/
src/lib.rs                                  human
src/main.rs                                 agent

# crates/set-dpi-awareness/
src/lib.rs                                  human

# crates/job-object/
src/lib.rs                                  agent

# server/
src/index.ts                                agent
src/core.ts                                 agent
src/protocol.ts                             agent
src/codec.ts                                agent
src/video.ts                                agent
src/kpm.ts                                  agent
src/strings.ts                              agent
src/selector.ts                             agent
src/persist.ts                              agent

# frontend/
debug.ts                                    human
global.css                                  human
global.effects.css                          human
global.tailwind.css                         human
index.html                                  human
index.tsx                                   human

# frontend/src/
api.ts                                      agent
streams.ts                                  agent
strings.ts                                  agent
strings-api.ts                              agent
ws.ts                                       agent
app.tsx                                     human

# frontend/src/components/
grid.tsx                                    human
marquee.tsx                                 agent

# frontend/src/widgets/
common.tsx                                  agent
index.tsx                                   agent

# frontend/src/kpm/
index.tsx                                   agent

# frontend/src/video/
chroma-key.ts                               agent
decoder.ts                                  agent
index.tsx                                   agent

# --- M3 artifacts (pending removal) ---

# live-video/ [M3]
Cargo.toml                                  human
src/main.rs                                 agent
src/lib.rs                                  agent
src/capture.rs                              agent
src/converter.rs                            agent
src/d3d11.rs                                agent
src/encoder.rs                              agent
src/encoder/debug.rs                        agent
src/encoder/helper.rs                       agent
src/resample.hlsl                           human
src/resample.rs                             human

# live-server/ [M3]
Cargo.toml                                  human
src/main.rs                                 agent
src/state.rs                                agent
src/constant.rs                             agent
src/windows.rs                              agent
src/message_pump.rs                         agent
src/vite_proxy.rs                           agent
src/video/buffer.rs                         agent
src/video/process.rs                        agent
src/video/routes.rs                         agent
src/video/ws.rs                             agent
src/strings/store.rs                        agent
src/strings/routes.rs                       agent
src/selector/config.rs                      agent
src/selector/manager.rs                     agent
src/selector/routes.rs                      agent
src/kpm/hook.rs                             agent
src/kpm/calculator.rs                       agent
src/kpm/ws.rs                               agent
src/youtube_music/manager.rs                agent
```
