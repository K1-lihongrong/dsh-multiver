import { defineConfig, devices } from "@playwright/test";

// 手册截图录屏专用配置（gif 手册图）。
// 与 playwright.config.js 的区别：
//   - testDir 指到 e2e/record，只有录屏 spec
//   - recordVideo 开启，尺寸 1280x800（与手册插图一致）
//   - workers=1（录屏必须串行，否则视频互相干扰）
//   - 关闭 retries（录到一半重试会产生多余视频）
//
// 用法：
//   pnpm exec playwright test --config=playwright.record.config.js
// 视频输出：e2e/record/.output/
export default defineConfig({
  testDir: "./e2e/record",
  timeout: 60000,
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [["list"]],
  use: {
    baseURL: "http://127.0.0.1:4173",
    viewport: { width: 1280, height: 800 },
    video: {
      mode: "on",
      size: { width: 1280, height: 800 },
    },
  },
  projects: [
    {
      name: "chromium",
      use: { ...devices["Desktop Chrome"], viewport: { width: 1280, height: 800 } },
    },
  ],
  webServer: {
    command: "pnpm preview --port 4173 --strictPort",
    url: "http://127.0.0.1:4173",
    reuseExistingServer: !process.env.CI,
    timeout: 60000,
  },
});
