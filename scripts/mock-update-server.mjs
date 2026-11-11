import http from 'node:http'
import fs from 'node:fs'
import crypto from 'node:crypto'

const payloadPath = process.argv[2]
const portFile = process.argv[3]
const tag = process.argv[4] || 'v0.4.1'
if (!payloadPath || !portFile) {
  console.error('usage: node mock-update-server.mjs <payload.exe> <port-file> [tag]')
  process.exit(2)
}

const payload = fs.readFileSync(payloadPath)
const digest = 'sha256:' + crypto.createHash('sha256').update(payload).digest('hex')

const server = http.createServer((req, res) => {
  if (req.url === '/repos/K1-lihongrong/dsh-multiver/releases/latest') {
    const body = JSON.stringify({
      tag_name: tag,
      html_url: 'http://127.0.0.1/mock-release',
      assets: [{
        name: 'dsh-multiver.exe',
        browser_download_url: `http://127.0.0.1:${server.address().port}/download.exe`,
        size: payload.length,
        digest,
      }],
    })
    res.writeHead(200, {
      'content-type': 'application/json',
      'content-length': Buffer.byteLength(body),
    })
    res.end(body)
    return
  }

  if (req.url === '/download.exe') {
    res.writeHead(200, {
      'content-type': 'application/octet-stream',
      'content-length': payload.length,
    })
    res.end(payload)
    return
  }

  res.writeHead(404, { 'content-type': 'text/plain' })
  res.end('not found')
})

server.listen(0, '127.0.0.1', () => {
  fs.writeFileSync(portFile, String(server.address().port), 'utf8')
})

for (const sig of ['SIGINT', 'SIGTERM']) {
  process.on(sig, () => server.close(() => process.exit(0)))
}