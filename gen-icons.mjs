import sharp from "sharp";
import fs from "node:fs";
import path from "node:path";

const svgPath = "icon-source.svg";
const outDir = "src-tauri/icons";
const svg = fs.readFileSync(svgPath);

// Tauri 需要的 PNG 尺寸
const pngTargets = [
  { size: 32,   name: "32x32.png" },
  { size: 128,  name: "128x128.png" },
  { size: 256,  name: "128x128@2x.png" },
  { size: 512,  name: "icon.png" },
  { size: 30,   name: "Square30x30Logo.png" },
  { size: 44,   name: "Square44x44Logo.png" },
  { size: 71,   name: "Square71x71Logo.png" },
  { size: 89,   name: "Square89x89Logo.png" },
  { size: 107,  name: "Square107x107Logo.png" },
  { size: 142,  name: "Square142x142Logo.png" },
  { size: 150,  name: "Square150x150Logo.png" },
  { size: 284,  name: "Square284x284Logo.png" },
  { size: 310,  name: "Square310x310Logo.png" },
  { size: 50,   name: "StoreLogo.png" },
];

async function genPngs() {
  fs.mkdirSync(outDir, { recursive: true });
  for (const t of pngTargets) {
    const buf = await sharp(svg).resize(t.size, t.size).png().toBuffer();
    fs.writeFileSync(path.join(outDir, t.name), buf);
    console.log("PNG", t.name, t.size + "x" + t.size);
  }
  // macOS 占位
  const icns = await sharp(svg).resize(512, 512).png().toBuffer();
  fs.writeFileSync(path.join(outDir, "icon.icns"), icns);
}

// 生成传统 BMP（DIB）格式的 ico，兼容 Windows 任务栏/资源管理器
async function makeDib(size) {
  const { data } = await sharp(svg)
    .resize(size, size, { fit: "contain", background: { r: 0, g: 0, b: 0, alpha: 0 } })
    .ensureAlpha()
    .raw()
    .toBuffer({ resolveWithObject: true });

  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0);
  header.writeInt32LE(size, 4);
  header.writeInt32LE(size * 2, 8);
  header.writeUInt16LE(1, 12);
  header.writeUInt16LE(32, 14);
  header.writeUInt32LE(0, 16);
  header.writeUInt32LE(size * size * 4, 20);

  const pixels = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) {
    const srcY = size - 1 - y;
    for (let x = 0; x < size; x++) {
      const s = (srcY * size + x) * 4;
      const d = (y * size + x) * 4;
      pixels[d] = data[s + 2];
      pixels[d + 1] = data[s + 1];
      pixels[d + 2] = data[s];
      pixels[d + 3] = data[s + 3];
    }
  }

  const rowBytes = Math.ceil(size / 32) * 4;
  const mask = Buffer.alloc(rowBytes * size, 0);
  return Buffer.concat([header, pixels, mask]);
}

async function genIco() {
  const sizes = [16, 24, 32, 48, 64, 128, 256];
  const blobs = [];
  for (const size of sizes) {
    blobs.push({ size, blob: await makeDib(size) });
  }

  const headerSize = 6 + blobs.length * 16;
  let offset = headerSize;
  const dirs = [];
  for (const { size, blob } of blobs) {
    const d = Buffer.alloc(16);
    d.writeUInt8(size >= 256 ? 0 : size, 0);
    d.writeUInt8(size >= 256 ? 0 : size, 1);
    d.writeUInt8(0, 2);
    d.writeUInt8(0, 3);
    d.writeUInt16LE(1, 4);
    d.writeUInt16LE(32, 6);
    d.writeUInt32LE(blob.length, 8);
    d.writeUInt32LE(offset, 12);
    offset += blob.length;
    dirs.push(d);
  }

  const icoHeader = Buffer.alloc(6);
  icoHeader.writeUInt16LE(0, 0);
  icoHeader.writeUInt16LE(1, 2);
  icoHeader.writeUInt16LE(blobs.length, 4);

  const out = Buffer.concat([icoHeader, ...dirs, ...blobs.map((b) => b.blob)]);
  fs.writeFileSync(path.join(outDir, "icon.ico"), out);
  console.log("ICO icon.ico (BMP format, 16/24/32/48/64/128/256):", out.length, "bytes");
}

async function main() {
  await genPngs();
  await genIco();
  console.log("DONE");
}

main().catch((e) => { console.error(e); process.exit(1); });
