// node mkbench.mjs <atmos.js> <out.html> — a measurement page: the same WGSL, each mode 300 frames at 960×540, ms per frame
import { readFileSync, writeFileSync } from "node:fs";
const [src, out] = process.argv.slice(2);
const js = readFileSync(src, "utf8");
const a = js.indexOf("const WGSL = /* wgsl */ `") + "const WGSL = /* wgsl */ `".length;
const wgsl = js.slice(a, js.indexOf("`;", a));
writeFileSync(out, `<!doctype html><meta charset="utf-8"><canvas width="960" height="540"></canvas><script>
const WGSL = ${JSON.stringify(wgsl)};
window.bench = (async () => {
  const adapter = await navigator.gpu.requestAdapter({ powerPreference: "high-performance" });
  const device = await adapter.requestDevice();
  const canvas = document.querySelector("canvas");
  const ctx = canvas.getContext("webgpu");
  const format = navigator.gpu.getPreferredCanvasFormat();
  ctx.configure({ device, format, alphaMode: "premultiplied" });
  const module = device.createShaderModule({ code: WGSL });
  const info = await module.getCompilationInfo();
  const errs = info.messages.filter(m => m.type === "error").map(m => m.lineNum + ": " + m.message);
  if (errs.length) return { errs };
  const pipeline = device.createRenderPipeline({ layout: "auto", vertex: { module, entryPoint: "vs" }, fragment: { module, entryPoint: "fs", targets: [{ format }] } });
  const ubuf = device.createBuffer({ size: 64, usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST });
  const bind = device.createBindGroup({ layout: pipeline.getBindGroupLayout(0), entries: [{ binding: 0, resource: { buffer: ubuf } }] });
  const res = {};
  for (const [name, mode] of [["torch", [1,0,0]], ["crypt", [0,1,0]], ["blood", [0,0,1]], ["all3", [1,1,1]], ["empty", [0,0,0]]]) {
    const run = async (n) => {
      const t0 = performance.now();
      for (let i = 0; i < n; i++) {
        device.queue.writeBuffer(ubuf, 0, new Float32Array([960, 540, i * 0.033, 0, .88,.66,.29,0, 1,.6,.24,0, ...mode, 0]));
        const tex = device.createTexture({ size: [960, 540], format, usage: GPUTextureUsage.RENDER_ATTACHMENT });
        const enc = device.createCommandEncoder();
        for (let k = 0; k < 40; k++) {
        const pass = enc.beginRenderPass({ colorAttachments: [{ view: tex.createView(), loadOp: "clear", clearValue: [0,0,0,0], storeOp: "store" }] });
        pass.setPipeline(pipeline); pass.setBindGroup(0, bind); pass.draw(3); pass.end();
        }
        device.queue.submit([enc.finish()]);
        await device.queue.onSubmittedWorkDone();
        tex.destroy();
      }
      return (performance.now() - t0) / n / 40;
    };
    await run(5);
    res[name] = +(await run(60)).toFixed(3);
  }
  // an empty pass: per-frame overhead without shader work
  return res;
})();
</script>`);
