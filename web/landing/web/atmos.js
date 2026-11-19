// WebGPU atmosphere behind the page — each launcher theme has its own:
//   "Torch" — light rays from a point above the screen on the left and dust glowing in the rays;
//   "Crypt" — fireflies in the theme colour wander and slowly blink, a faint cold light from above;
//   "Blood" — low fog drifts sideways, light pillars from items lying around shoot up from under the page, as in the game
//            (blue / purple / gold by rarity; tied to the page and moving slower than it).
// One full-screen fragment shader: the modes are blended by weights (a theme change flows over ~1.2 s), a mode with zero
// weight is not computed. Drawn at half resolution and at most 30 times a second; stops on a hidden tab. Colours —
// --glow and --ember of each theme (input.css; read once from the background layers). With "reduce motion" — one still frame (redrawn on scroll,
// resize and theme change). No WebGPU or adapter — the static CSS rays (.rays) stay: <html> without data-atmos="gpu".
(() => {
  const canvas = document.querySelector(".atmos-gpu");
  const root = document.documentElement;
  if (!canvas || !navigator.gpu) return;

  const SCALE = 0.5; // fraction of the screen resolution
  const FPS = 30;
  const MODES = { torch: [1, 0, 0], crypt: [0, 1, 0], blood: [0, 0, 1] };

  const WGSL = /* wgsl */ `
struct U {
  res: vec2f,     // canvas size in pixels
  time: f32,      // seconds
  scroll: f32,    // page scroll in canvas pixels
  glowT: vec4f,   // theme colours (rgb 0..1): light and particles of "Torch", "Crypt", "Blood" — read from CSS once
  emberT: vec4f,
  glowC: vec4f,
  emberC: vec4f,
  glowB: vec4f,
  emberB: vec4f,
  mode: vec4f,    // mode weights: x — torch, y — crypt, z — blood
};
@group(0) @binding(0) var<uniform> u: U;

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
  let p = array(vec2f(-1.0, -3.0), vec2f(-1.0, 1.0), vec2f(3.0, 1.0));
  return vec4f(p[i], 0.0, 1.0);
}

fn hash2(p: vec2f) -> vec2f {
  let q = vec2f(dot(p, vec2f(127.1, 311.7)), dot(p, vec2f(269.5, 183.3)));
  return fract(sin(q) * 43758.5453);
}
fn hash1(p: vec2f) -> f32 {
  return fract(sin(dot(p, vec2f(12.9898, 78.233))) * 43758.5453);
}
// smooth noise and its sum of octaves (fog)
fn vnoise(p: vec2f) -> f32 {
  let i = floor(p);
  let f = fract(p);
  let w = f * f * (3.0 - 2.0 * f);
  let a = hash1(i);
  let b = hash1(i + vec2f(1.0, 0.0));
  let c = hash1(i + vec2f(0.0, 1.0));
  let d = hash1(i + vec2f(1.0, 1.0));
  return mix(mix(a, b, w.x), mix(c, d, w.x), w.y);
}
fn fbm(p0: vec2f) -> f32 {
  var p = p0;
  var a = 0.5;
  var s = 0.0;
  for (var i = 0; i < 3; i++) {
    s += a * vnoise(p);
    p = p * 2.03 + vec2f(17.1, 9.2);
    a *= 0.5;
  }
  return s;
}

// ---------------------------------------------------------------- "Torch": rays and dust
// a fan of rays: wide ones sway by ±2.5° (~25 s), thin ones the opposite way (~34 s), each group has its own "breathing"
fn rays(p: vec2f, src: vec2f, t: f32) -> f32 {
  let d = p - src;
  let a = atan2(d.x, d.y); // 0 — down, plus — to the right
  let sway = 0.044 * sin(t * 0.25);
  let back = -0.044 * sin(t * 0.185 + 1.0);
  var r = 0.0;
  r += pow(0.5 + 0.5 * sin((a + sway) * 13.0 + t * 0.12 + 1.3), 6.0) * 0.55 * (0.65 + 0.35 * sin(t * 0.57 + 0.3));
  r += pow(0.5 + 0.5 * sin((a + back) * 23.0 - t * 0.09 + 0.4), 10.0) * 0.45 * (0.55 + 0.45 * sin(t * 0.83 + 2.0));
  r += pow(0.5 + 0.5 * sin((a + sway * 0.6) * 7.0 + t * 0.05 + 2.1), 4.0) * 0.5 * (0.7 + 0.3 * sin(t * 0.41 + 4.0));
  let sector = smoothstep(-0.35, 0.05, a) * (1.0 - smoothstep(0.95, 1.35, a));
  let dist = length(d) / length(u.res);
  let fall = exp(-dist * 2.2);
  let breathe = 0.72 + 0.28 * sin(t * 0.55);
  return r * sector * fall * breathe;
}
// dust layer: a grid of cells, in each a dust mote floats up and sways; it flickers
fn dust(p: vec2f, cell: f32, speed: f32, depth: f32, t: f32) -> f32 {
  let q = vec2f(p.x, p.y + u.scroll * depth + t * speed);
  let id = floor(q / cell);
  var acc = 0.0;
  for (var j = -1; j <= 1; j++) {
    for (var i = -1; i <= 1; i++) {
      let c = id + vec2f(f32(i), f32(j));
      let h = hash2(c);
      if (h.x > 0.55) { continue; }
      let wob = vec2f(sin(t * 0.4 + h.y * 6.28) * 0.18, cos(t * 0.3 + h.x * 6.28) * 0.1);
      let center = (c + 0.5 + (h - 0.5) * 0.7 + wob) * cell;
      let rad = cell * (0.02 + 0.05 * h.y * h.y);
      let k = 1.0 - smoothstep(0.0, rad, length(q - center));
      let tw = 0.55 + 0.45 * sin(t * (0.8 + 2.0 * h.x) + h.y * 40.0);
      acc += k * k * tw;
    }
  }
  return acc;
}
fn torch(p: vec2f, t: f32, s: f32) -> vec3f {
  let src = vec2f(u.res.x * 0.24, -u.res.y * 0.08);
  let r = rays(p, src, t);
  let haze = exp(-length((p - src) / u.res) * 3.2) * 0.22;
  var dd = dust(p, 70.0 * s, -9.0 * s, 0.10, t);
  dd += dust(p, 46.0 * s, -6.0 * s, 0.06, t) * 0.7;
  dd += dust(p, 30.0 * s, -3.5 * s, 0.03, t) * 0.45;
  let lit = clamp(r * 2.2 + haze * 2.0 + 0.12, 0.0, 1.0);
  return (r * 0.32 + haze * 0.5) * u.glowT.rgb + dd * lit * u.emberT.rgb * 0.9;
}

// ---------------------------------------------------------------- "Crypt": fireflies
// firefly layer: in some of the cells a light wanders along its loop and blinks (bright core + wide halo)
fn flies(p: vec2f, cell: f32, depth: f32, t: f32, s: f32) -> f32 {
  let q = vec2f(p.x, p.y + u.scroll * depth);
  let id = floor(q / cell);
  var acc = 0.0;
  for (var j = -1; j <= 1; j++) {
    for (var i = -1; i <= 1; i++) {
      let c = id + vec2f(f32(i), f32(j));
      let h = hash2(c);
      if (h.x > 0.42) { continue; }
      let g = hash2(c + vec2f(17.3, 5.1));
      let wander = vec2f(sin(t * (0.11 + 0.16 * g.x) + h.y * 6.28) + 0.4 * sin(t * (0.37 + 0.2 * g.y) + g.x * 9.0),
                         cos(t * (0.09 + 0.14 * g.y) + h.x * 6.28) + 0.4 * cos(t * (0.29 + 0.2 * g.x) + g.y * 7.0)) * 0.3;
      let center = (c + 0.5 + (g - 0.5) * 0.3 + wander) * cell;
      let d = length(q - center);
      let blink = smoothstep(0.1, 0.95, 0.5 + 0.5 * sin(t * (0.45 + 0.8 * g.y) + h.y * 40.0));
      let core = exp(-d * d / (2.0 * 1.7 * 1.7 * s * s));
      let halo = exp(-d / (10.0 * s)) * 0.32;
      acc += (core + halo) * blink;
    }
  }
  return acc;
}
fn crypt(p: vec2f, t: f32, s: f32) -> vec3f {
  let src = vec2f(u.res.x * 0.55, -u.res.y * 0.25);
  let cold = exp(-length((p - src) / u.res) * 2.6) * 0.16 * (0.85 + 0.15 * sin(t * 0.3));
  var f = flies(p, 120.0 * s, 0.08, t, s);
  f += flies(p, 80.0 * s, 0.04, t * 0.8 + 20.0, s * 0.7) * 0.55;
  return cold * u.glowC.rgb + f * u.emberC.rgb * 0.8;
}

// ---------------------------------------------------------------- "Blood": fog and light pillars from items
fn fog(p: vec2f, t: f32) -> f32 {
  let uv = p / u.res.y;
  let n = fbm(uv * vec2f(1.6, 3.2) + vec2f(t * 0.035, t * 0.008));
  let m = vnoise(uv * vec2f(0.9, 1.6) - vec2f(t * 0.02, -t * 0.004) + vec2f(5.0, 1.0)) * 0.85;
  let dens = smoothstep(0.3, 0.85, n * 0.65 + m * 0.55);
  let low = smoothstep(0.2, 1.0, p.y / u.res.y); // denser at the bottom of the screen
  return dens * (0.35 + 0.65 * low);
}
// colour by rarity: blue, purple, gold
fn rarity(h: f32) -> vec3f {
  if (h < 0.45) { return vec3f(0.30, 0.56, 1.0); }
  if (h < 0.82) { return vec3f(0.64, 0.36, 1.0); }
  return vec3f(1.0, 0.72, 0.30);
}
// pillars: up to three per page segment ~0.8 screens long; the base is at the bottom of the segment, the pillar goes up
fn beams(p: vec2f, t: f32, s: f32) -> vec3f {
  let q = vec2f(p.x, p.y + u.scroll * 0.45);
  let P = u.res.y * 0.8;
  let idx = floor(q.y / P);
  var acc = vec3f(0.0);
  for (var j = 0; j <= 1; j++) {
    let cell = idx + f32(j);
    for (var k = 0; k < 3; k++) {
      let h = hash2(vec2f(cell * 1.7, f32(k) * 7.1 + 3.0));
      if (h.y > 0.88) { continue; }
      let g = hash2(vec2f(cell * 3.1 + 11.0, f32(k) * 2.3));
      let bx = (0.04 + 0.92 * fract(h.x + f32(k) * 0.37)) * u.res.x;
      let base = (cell + 0.45 + 0.5 * g.x) * P;
      let height = u.res.y * (0.35 + 0.4 * g.y);
      let dy = base - q.y; // height above the base
      if (dy < -30.0 * s || dy > height) { continue; }
      let dx = abs(q.x - bx);
      let along = clamp(dy / height, 0.0, 1.0);
      let top = pow(1.0 - along, 1.5);
      let foot = smoothstep(-4.0 * s, 3.0 * s, dy);
      let shimmer = 0.72 + 0.28 * sin(dy * 0.07 / s - t * 2.4 + h.x * 30.0);
      let flicker = 0.85 + 0.15 * sin(t * (1.1 + h.y) + g.x * 9.0);
      var I = (exp(-dx / (0.8 * s)) * 1.1 + exp(-dx / (6.0 * s)) * 0.4 + exp(-dx / (24.0 * s)) * 0.1) * top * foot * shimmer;
      let e = length(vec2f(dx / (34.0 * s), dy / (7.0 * s)));
      I += exp(-e * e) * 0.55; // light spot on the floor
      acc += rarity(g.y * 0.6 + h.x * 0.4) * I * flicker;
    }
  }
  return acc;
}
fn blood(p: vec2f, t: f32, s: f32) -> vec3f {
  let f = fog(p, t);
  let fogCol = mix(u.glowB.rgb, vec3f(0.55, 0.5, 0.5), 0.35) * f * 0.42;
  let ash = dust(p, 60.0 * s, -5.0 * s, 0.05, t) * 0.35;
  return fogCol + beams(p, t, s) * 0.55 + ash * u.emberB.rgb * 0.5;
}

@fragment fn fs(@builtin(position) pos: vec4f) -> @location(0) vec4f {
  let p = pos.xy;
  let t = u.time;
  let s = u.res.y / 540.0; // detail scale from the screen height
  var c = vec3f(0.0);
  if (u.mode.x > 0.001) { c += torch(p, t, s) * u.mode.x; }
  if (u.mode.y > 0.001) { c += crypt(p, t, s) * u.mode.y; }
  if (u.mode.z > 0.001) { c += blood(p, t, s) * u.mode.z; }
  let a = clamp(max(max(c.r, c.g), c.b), 0.0, 1.0);
  return vec4f(min(c, vec3f(1.0)), a); // premultiplied: light over a dark background
}
`;

  const parseRgb = (s, fallback) => {
    s = (s || "").trim();
    let m = s.match(/^#([0-9a-f]{6})$/i);
    if (m) return [0, 2, 4].map((i) => parseInt(m[1].slice(i, i + 2), 16) / 255);
    m = s.match(/(\d+(?:\.\d+)?)[ ,]+(\d+(?:\.\d+)?)[ ,]+(\d+(?:\.\d+)?)/);
    return m ? [m[1], m[2], m[3]].map((v) => +v / 255) : fallback;
  };

  (async () => {
    await new Promise((r) => (document.readyState === "complete" ? r() : addEventListener("load", r, { once: true })));
    await new Promise((r) => (window.requestIdleCallback ? requestIdleCallback(r, { timeout: 800 }) : setTimeout(r, 100)));
    let adapter;
    try {
      adapter = await navigator.gpu.requestAdapter({ powerPreference: "low-power" });
    } catch {}
    if (!adapter) return;
    const device = await adapter.requestDevice();
    const ctx = canvas.getContext("webgpu");
    const format = navigator.gpu.getPreferredCanvasFormat();
    ctx.configure({ device, format, alphaMode: "premultiplied" });
    const module = device.createShaderModule({ code: WGSL });
    const pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module, entryPoint: "vs" },
      fragment: { module, entryPoint: "fs", targets: [{ format }] },
      primitive: { topology: "triangle-list" },
    });
    const ubuf = device.createBuffer({ size: 128, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
    const bind = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [{ binding: 0, resource: { buffer: ubuf } }] });
    const data = new Float32Array(32);
    root.dataset.atmos = "gpu"; // CSS hides the fallback rays

    const still = matchMedia("(prefers-reduced-motion: reduce)");
    // colours of the three themes — from the background layers (each has its own data-theme), once: each mode has its own colours
    const atmosEl = canvas.closest(".atmos") || root;
    const themeColors = (name) => {
      const el = document.querySelector(`.atmos-layer[data-theme="${name}"]`) || root;
      const cs = getComputedStyle(el);
      return [...parseRgb(cs.getPropertyValue("--glow"), [0.88, 0.66, 0.29]), 0, ...parseRgb(cs.getPropertyValue("--ember"), [1, 0.6, 0.24]), 0];
    };
    const colors = [...themeColors("torch"), ...themeColors("crypt"), ...themeColors("blood")];
    const targetMode = () => MODES[atmosEl.dataset.theme] || MODES.torch;
    let mode = [...targetMode()];
    // mode weights: towards the section theme over ~1.2 s (like the page colours)
    const stepMode = (dt) => {
      const tgt = targetMode();
      const k = still.matches ? 1 : Math.min(1, dt / 400);
      mode = mode.map((v, i) => (Math.abs(tgt[i] - v) < 0.002 ? tgt[i] : v + (tgt[i] - v) * k));
    };
    const resize = () => {
      const k = Math.min(2, devicePixelRatio || 1) * SCALE;
      canvas.width = Math.max(1, Math.round(innerWidth * k));
      canvas.height = Math.max(1, Math.round(innerHeight * k));
    };
    const draw = (time) => {
      const k = canvas.width / innerWidth;
      data.set([canvas.width, canvas.height, time / 1000, curY * k, ...colors, ...mode, 0]);
      device.queue.writeBuffer(ubuf, 0, data);
      const enc = device.createCommandEncoder();
      const pass = enc.beginRenderPass({
        colorAttachments: [{ view: ctx.getCurrentTexture().createView(), loadOp: "clear", clearValue: [0, 0, 0, 0], storeOp: "store" }],
      });
      pass.setPipeline(pipeline);
      pass.setBindGroup(0, bind);
      pass.draw(3);
      pass.end();
      device.queue.submit([enc.finish()]);
    };
    const drawStill = () => (stepMode(1e9), draw(40000));

    let running = false;
    let last = -1e9;
    // at rest — 30 frames per second; while the page moves (pillars and dust are tied to the scroll) or the modes are blending —
    // every monitor frame, otherwise the background lags behind the page and jerks (a shader frame is ~0.1 ms)
    let curY = scrollY;
    let moved = false;
    addEventListener("scroll", () => ((curY = scrollY), (moved = true)), { passive: true });
    const busy = () => moved || mode.some((v, i) => v !== targetMode()[i]);
    const frame = (time) => {
      if (still.matches || document.hidden || calm()) return (running = false);
      requestAnimationFrame(frame);
      if (!busy() && time - last < 1000 / FPS - 2) return;
      moved = false;
      stepMode(Math.min(100, time - last));
      last = time;
      draw(time);
    };
    const calm = () => "calm" in root.dataset; // nobody is using the window (site.js) — the last frame stays, no new ones are drawn
    const start = () => {
      if (calm()) return;
      if (still.matches || document.hidden) return drawStill(); // still frame
      if (!running) (running = true), requestAnimationFrame(frame);
    };
    resize();
    start();
    let rt;
    addEventListener("resize", () => {
      clearTimeout(rt);
      rt = setTimeout(() => (resize(), start(), still.matches && drawStill()), 120);
    });
    addEventListener("themechange", () => still.matches && drawStill());
    addEventListener("scroll", () => still.matches && drawStill(), { passive: true });
    document.addEventListener("visibilitychange", start);
    addEventListener("calmchange", start);
    still.addEventListener("change", start);
    device.lost.then(() => {
      running = false;
      delete root.dataset.atmos; // bring back the fallback rays
    });
  })();
})();
